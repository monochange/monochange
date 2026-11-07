//! Deterministic workspace graph evaluation.
//!
//! The graph decides which packages a change touches by classifying changed
//! files against workspace packages and walking the dependency edges between
//! them. Cargo edges come from `cargo metadata --format-version 1`, the
//! authoritative resolver: it includes unpublished members, crates nested at
//! any depth, `foo.workspace = true` inheritance, renamed dependencies, and
//! `[target.'cfg(..)'.dependencies]` sections. When Cargo is unavailable the
//! graph falls back to monochange's manifest discovery, and when even that
//! fails an unowned change runs every job — unknown must mean run, never skip.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command as ProcessCommand;

use glob::Pattern;
use monochange_config::CiJob;
use monochange_config::CiSettings;
use monochange_config::load_workspace_configuration;
use monochange_core::PackagePathMatch;
use monochange_core::PackagePathMatcher;
use monochange_core::PackageRecord;
use monochange_core::materialize_dependency_edges;

mod cargo;

use cargo::cargo_metadata_graph;

use crate::changeset_policy::normalize_changed_path;
use crate::discover_workspace;

/// Seconds in a minute, used when rendering duration suggestions.
const SECONDS_PER_MINUTE: f64 = 60.0;
/// Job-name fragments that identify a Rust build or test job.
const RUST_JOB_MARKERS: &[&str] = &["rust", "cargo", "clippy", "nextest", "miri", "semver"];
/// Speed advice shared by Rust jobs.
const RUST_SPEED_SUGGESTIONS: &[&str] = &[
	"Enable `sccache` or `Swatinem/rust-cache` to share compiled crate artifacts across CI runs.",
	"Switch test execution to `cargo-nextest` and shard slow integration suites across matrix runners.",
	"Use a faster linker (`mold` on Linux or `lld`) and scope `cargo test -p <crate>` to affected workspace packages.",
];
/// Speed advice shared by non-Rust jobs.
const GENERAL_SPEED_SUGGESTIONS: &[&str] = &[
	"Enable dependency and build caching for this job and parallelize independent test shards.",
	"Scope the job command to only affected workspace packages or directories rather than running the full workspace.",
];
/// Branch names that mean "the main branch" in this repository convention.
const MAIN_BRANCH_NAMES: &[&str] = &["main", "master", "refs/heads/main", "refs/heads/master"];
/// Directory roots that conventionally hold one package per child directory.
const PACKAGE_ROOTS: &[&str] = &["crates", "packages", "apps", "libs", "services"];
/// Glob metacharacters that distinguish a package glob from a literal name.
const GLOB_METACHARACTERS: &[char] = &['*', '?', '[', ']'];

/// Where a graph verdict came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VerdictSource {
	/// The job is absent from `[ci.jobs]`; unknown always runs.
	NoRule,
	/// A changed file is owned by a configured root package.
	RootDirect,
	/// A changed file is owned by a transitive dependency of a root.
	RootDependency,
	/// A changed file matched one of the job's `paths` globs.
	JobPath,
	/// A changed file is owned by nothing at all, so every job runs.
	UnownedChange,
	/// Nothing connected the change to this job.
	Unaffected,
}

/// The deterministic graph verdict for one job.
#[derive(Debug, Clone)]
pub(super) struct GraphVerdict {
	pub(super) run: bool,
	pub(super) source: VerdictSource,
	pub(super) reason: String,
	pub(super) affected_packages: Vec<String>,
}

/// How the dependency edges were derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphSource {
	/// Edges resolved by `cargo metadata`.
	CargoMetadata,
	/// Edges parsed from discovered manifests.
	DiscoveredManifests,
}

/// One workspace package the graph knows about.
#[derive(Debug, Clone, Default)]
struct GraphPackage {
	/// Directory relative to the workspace root; empty for root manifests.
	rel_dir: String,
}

/// Workspace packages, their aliases, and the dependency edges between them.
#[derive(Debug, Clone, Default)]
pub(super) struct WorkspaceGraph {
	/// Canonical package id → package details.
	packages: BTreeMap<String, GraphPackage>,
	/// Package name or configured id → canonical id.
	aliases: BTreeMap<String, String>,
	/// Canonical id → canonical ids it directly depends on.
	deps: BTreeMap<String, BTreeSet<String>>,
	/// Configured packages whose ownership follows their path settings.
	configured_matchers: Vec<(String, PackagePathMatcher)>,
	source: Option<GraphSource>,
}

/// One set of changed files evaluated against the workspace graph.
#[derive(Debug, Clone)]
pub(super) struct GraphContext {
	graph: WorkspaceGraph,
	/// Changed files that were not ignored and are not unconditional.
	triggering_files: Vec<String>,
	/// Canonical ids of packages owning at least one triggering changed file.
	owning_packages: BTreeSet<String>,
	/// Changed files owned by no package and claimed by no job's `paths`.
	unowned_files: Vec<String>,
}

impl WorkspaceGraph {
	/// An empty graph with no packages and no edges.
	fn empty() -> Self {
		Self::default()
	}

	/// Register a package with a workspace-relative directory.
	fn insert_package(&mut self, id: &str, rel_dir: String) {
		self.packages
			.insert(id.to_string(), GraphPackage { rel_dir });
	}

	/// Register `alias` as another name for `id`.
	fn insert_alias(&mut self, alias: &str, id: &str) {
		if alias != id {
			self.aliases.insert(alias.to_string(), id.to_string());
		}
	}

	/// Resolve a package name, alias, or id to its canonical id.
	fn canonical_id(&self, requested: &str) -> String {
		self.aliases
			.get(requested)
			.cloned()
			.unwrap_or_else(|| requested.to_string())
	}

	/// Every id and alias usable in `[ci.jobs]` package entries.
	pub(super) fn package_identifiers(&self) -> Vec<String> {
		let mut identifiers = self.packages.keys().cloned().collect::<Vec<_>>();

		identifiers.extend(self.aliases.keys().cloned());
		identifiers.sort();
		identifiers.dedup();

		identifiers
	}

	/// Expand `[ci.jobs.<id>].packages` entries against every workspace package.
	///
	/// Globs match both canonical ids and package names, so
	/// `leptoskit_plugin_*` picks up a newly added crate with zero config edits.
	fn expand_roots(&self, patterns: &[String]) -> Vec<String> {
		let identifiers = self.package_identifiers();
		let mut roots = BTreeSet::new();

		for pattern in patterns {
			if contains_glob_metacharacters(pattern) {
				roots.extend(
					identifiers
						.iter()
						.filter(|identifier| glob_match(identifier, pattern))
						.map(|identifier| self.canonical_id(identifier)),
				);
			} else {
				let canonical = self.canonical_id(pattern);

				if self.packages.contains_key(&canonical) {
					roots.insert(canonical);
				}
			}
		}

		roots.into_iter().collect()
	}

	/// Transitive workspace dependencies of `root`, following every edge kind.
	fn transitive_dependencies(&self, root: &str) -> BTreeSet<String> {
		let mut visited = BTreeSet::new();
		let mut queue: Vec<String> = self
			.deps
			.get(root)
			.map(|direct| direct.iter().cloned().collect())
			.unwrap_or_default();

		while let Some(package) = queue.pop() {
			if !visited.insert(package.clone()) {
				continue;
			}

			if let Some(next) = self.deps.get(&package) {
				queue.extend(next.iter().cloned());
			}
		}

		visited
	}

	/// Canonical ids of every package owning `file`.
	///
	/// Configured packages keep their matcher semantics (`additional_paths`,
	/// `ignored_paths`); every other package owns the tree under its directory.
	fn owners_of(&self, file: &str) -> Vec<String> {
		let mut owners: BTreeSet<String> = self
			.configured_matchers
			.iter()
			.filter(|(_, matcher)| {
				matches!(matcher.classify(Path::new(file)), PackagePathMatch::Touched)
			})
			.map(|(id, _)| id.clone())
			.collect();

		for (id, package) in &self.packages {
			if self
				.configured_matchers
				.iter()
				.any(|(matcher_id, _)| matcher_id == id)
			{
				continue;
			}

			if package_dir_matches(&package.rel_dir, file) {
				owners.insert(id.clone());
			}
		}

		owners.into_iter().collect()
	}

	/// Merge `other` into `self`, aliasing same-directory duplicates.
	fn merge(&mut self, other: WorkspaceGraph) {
		let mut directory_owners: BTreeMap<String, String> = self
			.packages
			.iter()
			.filter(|(_, package)| !package.rel_dir.is_empty())
			.map(|(id, package)| (package.rel_dir.clone(), id.clone()))
			.collect();
		let mut remapped: BTreeMap<String, String> = BTreeMap::new();

		for (id, package) in other.packages {
			let existing = if package.rel_dir.is_empty() {
				None
			} else {
				directory_owners.get(&package.rel_dir).cloned()
			};

			if let Some(canonical) = existing {
				remapped.insert(id, canonical);
			} else {
				if !package.rel_dir.is_empty() {
					directory_owners.insert(package.rel_dir.clone(), id.clone());
				}
				self.packages.insert(id.clone(), package);
			}
		}

		let resolve =
			|id: &str| -> String { remapped.get(id).cloned().unwrap_or_else(|| id.to_string()) };

		for (alias, target) in other.aliases {
			self.aliases.insert(alias, resolve(&target));
		}

		for (from, to) in &remapped {
			self.aliases.insert(from.clone(), to.clone());
		}

		for (dependent, dependencies) in other.deps {
			let bucket = self.deps.entry(resolve(&dependent)).or_default();

			for dependency in dependencies {
				bucket.insert(resolve(&dependency));
			}
		}

		for (id, matcher) in other.configured_matchers {
			let canonical = resolve(&id);

			if !self
				.configured_matchers
				.iter()
				.any(|(matcher_id, _)| *matcher_id == canonical)
			{
				self.configured_matchers.push((canonical, matcher));
			}
		}
	}

	/// Label describing how the dependency edges were derived.
	pub(super) fn source_label(&self) -> &'static str {
		match self.source {
			Some(GraphSource::CargoMetadata) => "cargo metadata",
			Some(GraphSource::DiscoveredManifests) => "manifest discovery",
			None => "no dependency source",
		}
	}
}

impl GraphContext {
	/// Packages owning at least one triggering changed file.
	pub(super) fn affected_packages(&self) -> Vec<String> {
		self.owning_packages.iter().cloned().collect()
	}

	/// Whether every changed file was ignored, so no package was affected.
	pub(super) fn ignored_only(&self) -> bool {
		self.owning_packages.is_empty() && self.unowned_files.is_empty()
	}
}

/// Test whether a package entry uses glob syntax rather than a literal name.
fn contains_glob_metacharacters(pattern: &str) -> bool {
	pattern
		.chars()
		.any(|character| GLOB_METACHARACTERS.contains(&character))
}

/// Normalize changed paths and drop empty entries.
fn normalized_files(changed_files: &[String]) -> Vec<String> {
	changed_files
		.iter()
		.map(|path| normalize_changed_path(path))
		.filter(|path| !path.is_empty())
		.collect()
}

/// Build the workspace graph for `root`.
///
/// Cargo metadata is authoritative for Cargo workspaces; monochange's
/// discovery adds every other ecosystem's packages and edges. Each source is
/// best effort: a missing or broken source degrades the graph instead of
/// failing the decision, and unowned changes still run every job.
pub(super) fn build_workspace_graph(root: &Path) -> WorkspaceGraph {
	let mut graph = WorkspaceGraph::empty();

	if let Some(cargo) = cargo_metadata_graph(root) {
		graph.source = Some(GraphSource::CargoMetadata);
		graph.merge(cargo);
	}

	graph.merge(discovered_graph(root));

	if graph.source.is_none() && !graph.deps.is_empty() {
		graph.source = Some(GraphSource::DiscoveredManifests);
	}

	graph
}

/// Evaluate changed files against the workspace graph and `[ci]` settings.
///
/// Per file: a job-path claim wins over `ignore_paths` (a job that explicitly
/// claims `docs/**` still triggers on docs changes), `ignore_paths` drops the
/// rest, and unconditional paths are left to rule 1. Everything left triggers:
/// owned by a package, or unowned when no job claims it either.
pub(super) fn evaluate_graph_context(
	root: &Path,
	changed_files: &[String],
	settings: &CiSettings,
) -> GraphContext {
	let files = normalized_files(changed_files);
	let graph = build_workspace_graph(root);
	let job_paths: Vec<&Vec<String>> = settings
		.jobs
		.values()
		.map(|job| &job.paths)
		.filter(|paths| !paths.is_empty())
		.collect();
	let mut context = GraphContext {
		graph,
		triggering_files: Vec::new(),
		owning_packages: BTreeSet::new(),
		unowned_files: Vec::new(),
	};

	for file in &files {
		let claimed_by_job = job_paths
			.iter()
			.any(|paths| matches_any_glob(file, paths).is_some());

		if !claimed_by_job && matches_any_glob(file, &settings.ignore_paths).is_some() {
			continue;
		}

		if matches_any_glob(file, &settings.unconditional_paths).is_some() {
			continue;
		}

		context.triggering_files.push(file.clone());

		let owners = context.graph.owners_of(file);

		if owners.is_empty() {
			if !claimed_by_job {
				context.unowned_files.push(file.clone());
			}
		} else {
			context.owning_packages.extend(owners);
		}
	}

	context
}

/// Evaluate the deterministic graph verdict for one configured job.
///
/// This covers decision rules 4–7: root packages and their transitive
/// dependencies, explicit job paths, unowned changes, and the skip verdict.
/// Unconditional context, `always`, and learned rules are evaluated by the
/// caller before this function runs.
pub(super) fn evaluate_graph_decision_for_job(
	job_id: &str,
	job: Option<&CiJob>,
	context: &GraphContext,
) -> GraphVerdict {
	let Some(job) = job else {
		return no_rule_verdict(job_id);
	};

	let affected = context.affected_packages();

	if let Some(verdict) = root_verdict(job, context) {
		return verdict;
	}

	if let Some(file) = context
		.triggering_files
		.iter()
		.find(|file| matches_any_glob(file, &job.paths).is_some())
	{
		return run_verdict(
			VerdictSource::JobPath,
			format!("changed file `{file}` matched job path pattern"),
			affected.clone(),
		);
	}

	if let Some(file) = context.unowned_files.first() {
		return run_verdict(
			VerdictSource::UnownedChange,
			format!("unowned changed file `{file}` runs every job"),
			affected.clone(),
		);
	}

	skip_verdict(job_id, job, &affected)
}

/// Verdict for a job with no `[ci.jobs]` entry: unknown always runs.
fn no_rule_verdict(job_id: &str) -> GraphVerdict {
	run_verdict(
		VerdictSource::NoRule,
		format!("no ci rule for `{job_id}`"),
		Vec::new(),
	)
}

/// Rules 4a and 4b: a root owns a changed file, or depends on one that does.
fn root_verdict(job: &CiJob, context: &GraphContext) -> Option<GraphVerdict> {
	for root in context.graph.expand_roots(&job.packages) {
		if context.owning_packages.contains(&root) {
			return Some(run_verdict(
				VerdictSource::RootDirect,
				format!("changed files affect root package `{root}`"),
				vec![root],
			));
		}

		let dependency = context.owning_packages.iter().find(|package| {
			context
				.graph
				.transitive_dependencies(&root)
				.contains(*package)
		});

		if let Some(dependency) = dependency {
			return Some(run_verdict(
				VerdictSource::RootDependency,
				format!("changed files affect `{dependency}`, a dependency of root `{root}`"),
				vec![dependency.clone()],
			));
		}
	}

	None
}

/// Rule 7: nothing connected the change to this job.
fn skip_verdict(job_id: &str, job: &CiJob, affected: &[String]) -> GraphVerdict {
	let reason = if affected.is_empty() {
		"every changed file is ignored by `[ci] ignore_paths` or already unconditional".to_string()
	} else {
		format!(
			"changed files affect {} but `{job_id}` covers roots [{}] and paths [{}]",
			join_quoted(affected),
			job.packages.join(", "),
			job.paths.join(", ")
		)
	};

	GraphVerdict {
		run: false,
		source: VerdictSource::Unaffected,
		reason,
		affected_packages: Vec::new(),
	}
}

/// Build a "run" verdict.
fn run_verdict(
	source: VerdictSource,
	reason: String,
	affected_packages: Vec<String>,
) -> GraphVerdict {
	GraphVerdict {
		run: true,
		source,
		reason,
		affected_packages,
	}
}

/// Join package ids as backticked list text.
fn join_quoted(values: &[String]) -> String {
	values
		.iter()
		.map(|value| format!("`{value}`"))
		.collect::<Vec<_>>()
		.join(", ")
}

/// Build the graph from configured and discovered (non-Cargo-resolved) packages.
fn discovered_graph(root: &Path) -> WorkspaceGraph {
	let mut graph = WorkspaceGraph::empty();

	if let Ok(configuration) = load_workspace_configuration(root) {
		for package in &configuration.packages {
			let rel_dir = package
				.path
				.to_string_lossy()
				.replace('\\', "/")
				.trim_end_matches('/')
				.to_string();

			graph.insert_package(&package.id, rel_dir);
			graph.configured_matchers.push((
				package.id.clone(),
				PackagePathMatcher::new(
					&package.id,
					&package.path,
					&package.additional_paths,
					&package.ignored_paths,
				),
			));
		}
	}

	let Ok(discovery) = discover_workspace(root) else {
		return graph;
	};

	let mut discovery_aliases = BTreeMap::new();

	for record in &discovery.packages {
		let config_id = record
			.metadata
			.get("config_id")
			.cloned()
			.unwrap_or_else(|| record.id.clone());
		let rel_dir = relative_package_dir(record, &discovery.workspace_root);

		discovery_aliases.insert(record.name.clone(), config_id.clone());
		discovery_aliases.insert(record.id.clone(), config_id.clone());
		graph.insert_package(&config_id, rel_dir);
	}

	for (alias, id) in &discovery_aliases {
		graph.insert_alias(alias, id);
	}

	for edge in materialize_dependency_edges(&discovery.packages) {
		let from = resolve_discovered_id(&edge.from_package_id, &discovery_aliases);
		let to = resolve_discovered_id(&edge.to_package_id, &discovery_aliases);

		graph.deps.entry(from).or_default().insert(to);
	}

	graph
}

/// Map a discovered package id or name to its configuration-level id.
fn resolve_discovered_id(id: &str, aliases: &BTreeMap<String, String>) -> String {
	aliases.get(id).cloned().unwrap_or_else(|| id.to_string())
}

/// Compute a discovered package's directory relative to the workspace root.
///
/// Discovery reports canonicalized manifest paths while callers may pass a
/// symlinked root (macOS `/var` vs `/private/var`), so both spellings are
/// tried before giving up.
pub(super) fn relative_package_dir(record: &PackageRecord, workspace_root: &Path) -> String {
	let Some(dir) = record.manifest_path.parent() else {
		return String::new();
	};

	let candidates = [Some(dir.to_path_buf()), dir.canonicalize().ok()];

	for candidate in candidates.into_iter().flatten() {
		let stripped = candidate.strip_prefix(workspace_root).ok().or_else(|| {
			workspace_root
				.canonicalize()
				.ok()
				.and_then(|canonical| candidate.strip_prefix(canonical).ok())
		});

		if let Some(relative) = stripped {
			return relative.to_string_lossy().replace('\\', "/");
		}
	}

	String::new()
}

/// Test whether a changed file belongs to a package directory.
pub(super) fn package_dir_matches(rel_dir: &str, file: &str) -> bool {
	if rel_dir.is_empty() {
		return false;
	}

	file == rel_dir || file.starts_with(&format!("{rel_dir}/"))
}

/// Detect whether a job builds Rust, which gets a longer duration budget.
pub(super) fn is_rust_job_or_context(job: &str, changed_files: &[String], root: &Path) -> bool {
	let lower_job = job.to_ascii_lowercase();

	if RUST_JOB_MARKERS
		.iter()
		.any(|marker| lower_job.contains(marker))
	{
		return true;
	}

	if changed_files.iter().any(|file| is_rust_path(file)) {
		return true;
	}

	root.join("Cargo.toml").is_file() && lower_job.contains("test")
}

/// Test whether a changed path belongs to the Rust toolchain.
fn is_rust_path(file: &str) -> bool {
	Path::new(file)
		.extension()
		.is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
		|| file.ends_with("Cargo.toml")
		|| file == "Cargo.lock"
}

/// Build duration red-flag suggestions for a job that ran too long.
#[must_use]
pub fn build_speed_suggestions(
	job: &str,
	is_rust: bool,
	duration_seconds: u64,
	threshold_minutes: u64,
) -> Vec<String> {
	let duration_minutes = duration_seconds as f64 / SECONDS_PER_MINUTE;
	let mut suggestions = vec![format!(
		"Job `{job}` took {duration_minutes:.1}m, exceeding the {threshold_minutes}m threshold."
	)];
	let defaults = if is_rust {
		RUST_SPEED_SUGGESTIONS
	} else {
		GENERAL_SPEED_SUGGESTIONS
	};

	suggestions.extend(defaults.iter().map(|suggestion| (*suggestion).to_string()));

	suggestions
}

/// Seconds in a minute as an integer, used for threshold comparisons.
const SECONDS_PER_MINUTE_INT: u64 = 60;

/// Choose the duration red-flag threshold for a job.
pub(super) fn duration_threshold_minutes(
	is_rust: bool,
	general_minutes: u64,
	rust_minutes: u64,
) -> u64 {
	if is_rust {
		rust_minutes
	} else {
		general_minutes
	}
}

/// Test whether a duration exceeds its threshold in minutes.
pub(super) fn exceeds_duration_threshold(duration_seconds: u64, threshold_minutes: u64) -> bool {
	duration_seconds > threshold_minutes * SECONDS_PER_MINUTE_INT
}

/// Build red-flag suggestions when an observed duration exists and is too long.
pub(super) fn optional_duration_suggestions(
	job: &str,
	is_rust: bool,
	observed_seconds: Option<u64>,
	threshold_minutes: u64,
) -> Vec<String> {
	match observed_seconds {
		Some(seconds) if exceeds_duration_threshold(seconds, threshold_minutes) => {
			build_speed_suggestions(job, is_rust, seconds, threshold_minutes)
		}

		_ => Vec::new(),
	}
}

/// Resolve changed files from git when they were not passed explicitly.
pub(super) fn detect_changed_files_from_git(
	root: &Path,
	base: Option<&str>,
	head: Option<&str>,
) -> Vec<String> {
	let mut args = vec!["diff".to_string(), "--name-only".to_string()];

	args.push(diff_specification(base, head));

	let Ok(output) = ProcessCommand::new("git")
		.current_dir(root)
		.args(&args)
		.output()
	else {
		// patch-coverage:ignore-start -- spawning `git` only fails when the binary is missing from PATH.
		return Vec::new();
		// patch-coverage:ignore-end
	};

	if !output.status.success() {
		return Vec::new();
	}

	String::from_utf8_lossy(&output.stdout)
		.lines()
		.map(normalize_changed_path)
		.filter(|line| !line.is_empty())
		.collect()
}

/// Build the `git diff` revision argument for the supplied refs.
pub(super) fn diff_specification(base: Option<&str>, head: Option<&str>) -> String {
	match (base, head) {
		(Some(base_ref), Some(head_ref)) => format!("{base_ref}...{head_ref}"),
		(Some(base_ref), None) => base_ref.to_string(),
		(None, Some(head_ref)) => head_ref.to_string(),
		(None, None) => "HEAD".to_string(),
	}
}

/// Return the first pattern that matches `value`, either as a glob or literally.
pub(super) fn matches_any_glob(value: &str, patterns: &[String]) -> Option<String> {
	patterns
		.iter()
		.find(|pattern| glob_match(value, pattern))
		.cloned()
}

/// Test one glob pattern, falling back to literal equality.
pub(super) fn glob_match(value: &str, pattern: &str) -> bool {
	if value == pattern {
		return true;
	}

	Pattern::new(pattern).is_ok_and(|compiled| compiled.matches(value))
}

/// Detect the main branch from either the branch name or the full git ref.
pub(super) fn is_main_branch(branch: Option<&str>, git_ref: Option<&str>) -> bool {
	[branch, git_ref]
		.into_iter()
		.flatten()
		.any(is_main_branch_name)
}

/// Test one branch name against the main-branch aliases.
fn is_main_branch_name(value: &str) -> bool {
	let trimmed = value.trim();

	MAIN_BRANCH_NAMES.contains(&trimmed)
}

/// Extract learned override globs from the files that caused a failure.
///
/// A false negative usually repeats: the same file or package directory keeps
/// being skipped. Widening the match to the package directory catches siblings
/// without learning the whole repository.
pub(super) fn derive_learned_patterns_from_files(changed_files: &[String]) -> Vec<String> {
	let patterns = learned_patterns(changed_files);

	if patterns.is_empty() {
		return vec!["*".to_string()];
	}

	patterns.into_iter().collect()
}

/// Collect normalized file and directory patterns.
fn learned_patterns(changed_files: &[String]) -> BTreeSet<String> {
	let mut patterns = BTreeSet::new();

	for file in changed_files {
		let normalized = normalize_changed_path(file);

		if normalized.is_empty() {
			continue;
		}

		patterns.insert(normalized.clone());
		extend_package_patterns(&mut patterns, &normalized);
	}

	patterns
}

/// Add package-directory globs for known monorepo layouts.
fn extend_package_patterns(patterns: &mut BTreeSet<String>, normalized: &str) {
	let segments: Vec<&str> = normalized.split('/').collect();
	let is_package_root = segments
		.first()
		.is_some_and(|segment| PACKAGE_ROOTS.contains(segment));

	if is_package_root && let (Some(prefix), Some(name)) = (segments.first(), segments.get(1)) {
		patterns.insert(format!("{prefix}/{name}/*"));
		patterns.insert(format!("{prefix}/{name}/**"));

		return;
	}

	if let Some((parent, _)) = normalized.rsplit_once('/') {
		patterns.insert(format!("{parent}/*"));
	}
}
