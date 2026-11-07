//! Deterministic workspace graph evaluation.
//!
//! The graph decides which packages a change touches by classifying changed
//! files against configured package paths, discovered manifests, and the
//! transitive dependency edges between them. This is the fallback whenever an
//! external provider is unavailable, so it must never panic and must stay
//! explainable in the decision reason.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command as ProcessCommand;

use glob::Pattern;
use monochange_config::load_workspace_configuration;
use monochange_core::DependencyEdge;
use monochange_core::DiscoveryReport;
use monochange_core::PackagePathMatch;
use monochange_core::PackagePathMatcher;
use monochange_core::PackageRecord;
use monochange_core::materialize_dependency_edges;

use crate::changeset_policy::normalize_changed_path;
use crate::discover_workspace;

/// Outcome of a graph decision: whether to run, why, and which packages matched.
pub(super) type GraphVerdict = (bool, String, Vec<String>);

/// Seconds in a minute, used when rendering duration suggestions.
const SECONDS_PER_MINUTE: f64 = 60.0;
/// Job-name fragments that identify a Rust build or test job.
const RUST_JOB_MARKERS: &[&str] = &["rust", "cargo", "clippy", "nextest", "miri", "semver"];
/// Job-name fragments that identify a documentation-only job.
const DOC_JOB_MARKERS: &[&str] = &["doc", "spell", "markdown"];
/// File extensions treated as documentation.
const DOC_EXTENSIONS: &[&str] = &["md", "mdx", "txt"];
/// Path prefixes treated as documentation or metadata.
const DOC_PATH_PREFIXES: &[&str] = &["docs/", ".changeset/", ".monochange/"];
/// Exact file names treated as repository metadata.
const DOC_FILE_NAMES: &[&str] = &["license", ".gitignore"];
/// Directory roots that conventionally hold one package per child directory.
const PACKAGE_ROOTS: &[&str] = &["crates", "packages", "apps", "libs", "services"];
/// Branch names that mean "the main branch" in this repository convention.
const MAIN_BRANCH_NAMES: &[&str] = &["main", "master", "refs/heads/main", "refs/heads/master"];
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

/// Resolved workspace graph for one set of changed files.
#[derive(Debug, Clone, Default)]
pub(super) struct WorkspaceGraphContext {
	pub(super) affected_package_ids: BTreeSet<String>,
	pub(super) package_names_to_ids: BTreeMap<String, String>,
	pub(super) docs_or_ignored_only: bool,
}

/// Accumulates package identity maps and affected packages while scanning.
struct GraphBuilder {
	direct_affected: BTreeSet<String>,
	package_names_to_ids: BTreeMap<String, String>,
}

impl GraphBuilder {
	/// Start from empty state.
	fn new() -> Self {
		Self {
			direct_affected: BTreeSet::new(),
			package_names_to_ids: BTreeMap::new(),
		}
	}

	/// Add packages declared in `monochange.toml`.
	fn add_configured_packages(&mut self, root: &Path, files: &[String]) {
		let Ok(configuration) = load_workspace_configuration(root) else {
			return;
		};

		for package in &configuration.packages {
			self.package_names_to_ids
				.insert(package.id.clone(), package.id.clone());

			let matcher = PackagePathMatcher::new(
				&package.id,
				&package.path,
				&package.additional_paths,
				&package.ignored_paths,
			);

			self.mark_matching_files(&package.id, &matcher, files);
		}
	}

	/// Mark a configured package as affected when any file classifies as touched.
	fn mark_matching_files(
		&mut self,
		package_id: &str,
		matcher: &PackagePathMatcher,
		files: &[String],
	) {
		for file in files {
			if matches!(matcher.classify(Path::new(file)), PackagePathMatch::Touched) {
				self.direct_affected.insert(package_id.to_string());
			}
		}
	}

	/// Add auto-discovered packages and propagate affected dependents.
	fn add_discovered_packages(&mut self, root: &Path, files: &[String]) {
		let Ok(discovery) = discover_workspace(root) else {
			return;
		};

		for record in &discovery.packages {
			self.add_discovered_package(record, &discovery.workspace_root, files);
		}

		self.propagate_dependents(&discovery);
	}

	/// Register one discovered package and mark it affected by `files`.
	fn add_discovered_package(
		&mut self,
		record: &PackageRecord,
		workspace_root: &Path,
		files: &[String],
	) {
		let config_id = record
			.metadata
			.get("config_id")
			.cloned()
			.unwrap_or_else(|| record.id.clone());

		self.package_names_to_ids
			.insert(record.name.clone(), config_id.clone());
		self.package_names_to_ids
			.insert(record.id.clone(), config_id.clone());

		let rel_dir = relative_package_dir(record, workspace_root);

		for file in files {
			if package_dir_matches(&rel_dir, file) {
				self.direct_affected.insert(config_id.clone());
			}
		}
	}

	/// Walk dependency edges until no further dependents become affected.
	fn propagate_dependents(&mut self, discovery: &DiscoveryReport) {
		let edges = materialize_dependency_edges(&discovery.packages);
		let mut changed = true;

		while changed {
			changed = self.propagate_once(&edges);
		}
	}

	/// Run one dependency propagation pass, reporting whether anything changed.
	fn propagate_once(&mut self, edges: &[DependencyEdge]) -> bool {
		let mut changed = false;

		for edge in edges {
			let from_id = self.resolve_id(&edge.from_package_id);
			let to_id = self.resolve_id(&edge.to_package_id);

			if self.direct_affected.contains(&to_id) && self.direct_affected.insert(from_id) {
				changed = true;
			}
		}

		changed
	}

	/// Map a manifest-level package id to its configuration-level id.
	fn resolve_id(&self, id: &str) -> String {
		self.package_names_to_ids
			.get(id)
			.cloned()
			.unwrap_or_else(|| id.to_string())
	}

	/// Finish the scan with the documentation-only signal.
	fn finish(self, all_docs_or_non_code: bool) -> WorkspaceGraphContext {
		WorkspaceGraphContext {
			docs_or_ignored_only: all_docs_or_non_code && self.direct_affected.is_empty(),
			affected_package_ids: self.direct_affected,
			package_names_to_ids: self.package_names_to_ids,
		}
	}
}

/// Normalize changed paths and drop empty entries.
fn normalized_files(changed_files: &[String]) -> Vec<String> {
	changed_files
		.iter()
		.map(|path| normalize_changed_path(path))
		.filter(|path| !path.is_empty())
		.collect()
}

/// Resolve changed files to affected packages using the workspace graph.
pub(super) fn evaluate_workspace_graph_context(
	root: &Path,
	changed_files: &[String],
) -> WorkspaceGraphContext {
	let files = normalized_files(changed_files);

	if files.is_empty() {
		return WorkspaceGraphContext {
			docs_or_ignored_only: true,
			..WorkspaceGraphContext::default()
		};
	}

	let all_docs_or_non_code = files.iter().all(|path| is_doc_or_metadata_path(path));
	let mut builder = GraphBuilder::new();

	builder.add_configured_packages(root, &files);
	builder.add_discovered_packages(root, &files);

	builder.finish(all_docs_or_non_code)
}

/// Compute a discovered package's directory relative to the workspace root.
fn relative_package_dir(record: &PackageRecord, workspace_root: &Path) -> String {
	record
		.manifest_path
		.parent()
		.and_then(|dir| dir.strip_prefix(workspace_root).ok())
		.map(|dir| dir.to_string_lossy().replace('\\', "/"))
		.unwrap_or_default()
}

/// Test whether a changed file belongs to a package directory.
fn package_dir_matches(rel_dir: &str, file: &str) -> bool {
	if rel_dir.is_empty() || is_doc_or_metadata_path(file) {
		return false;
	}

	file == rel_dir || file.starts_with(&format!("{rel_dir}/"))
}

/// Decide whether a job should run, preferring explicit job scope over heuristics.
pub(super) fn evaluate_graph_decision_for_job(
	job: &str,
	changed_files: &[String],
	job_paths: &[String],
	job_packages: &[String],
	graph: &WorkspaceGraphContext,
) -> GraphVerdict {
	if changed_files.is_empty() {
		return skip_verdict(format!("no changed files detected for job `{job}`"));
	}

	job_path_verdict(job, changed_files, job_paths, graph)
		.or_else(|| job_package_verdict(job, job_packages, graph))
		.or_else(|| docs_only_verdict(job, graph))
		.or_else(|| job_named_package_verdict(job, graph))
		.or_else(|| affected_packages_verdict(graph))
		.unwrap_or_else(|| conservative_verdict(job))
}

/// Match changed files against explicit `--job-paths` globs.
fn job_path_verdict(
	job: &str,
	changed_files: &[String],
	job_paths: &[String],
	graph: &WorkspaceGraphContext,
) -> Option<GraphVerdict> {
	if job_paths.is_empty() {
		return None;
	}

	for file in changed_files {
		if let Some(matched_glob) = matches_any_glob(file, job_paths) {
			let packages = graph.affected_package_ids.iter().cloned().collect();

			return Some(run_verdict(
				format!("changed file `{file}` matched job path pattern `{matched_glob}`"),
				packages,
			));
		}
	}

	Some(skip_verdict(format!(
		"no changed files matched configured `--job-paths` for `{job}`"
	)))
}

/// Match requested `--job-packages` against affected packages.
fn job_package_verdict(
	job: &str,
	job_packages: &[String],
	graph: &WorkspaceGraphContext,
) -> Option<GraphVerdict> {
	if job_packages.is_empty() {
		return None;
	}

	let matched: Vec<String> = job_packages
		.iter()
		.map(|requested| graph.resolve_package_id(requested))
		.filter(|resolved| graph.affected_package_ids.contains(resolved))
		.collect();

	if !matched.is_empty() {
		return Some(run_verdict(
			format!(
				"affected workspace package(s) `{}` matched job `{job}`",
				matched.join(", ")
			),
			matched,
		));
	}

	Some(skip_verdict(format!(
		"none of the job packages ({}) were affected",
		job_packages.join(", ")
	)))
}

/// Skip non-documentation jobs when only docs or metadata changed.
fn docs_only_verdict(job: &str, graph: &WorkspaceGraphContext) -> Option<GraphVerdict> {
	if !graph.docs_or_ignored_only {
		return None;
	}

	let lower_job = job.to_ascii_lowercase();

	if DOC_JOB_MARKERS
		.iter()
		.any(|marker| lower_job.contains(marker))
	{
		return Some(run_verdict(
			format!("documentation job `{job}` matched documentation-only changes"),
			Vec::new(),
		));
	}

	Some(skip_verdict(format!(
		"skipping `{job}` because only documentation or non-code metadata files changed"
	)))
}

/// Run only when a package named directly by the job is affected.
fn job_named_package_verdict(job: &str, graph: &WorkspaceGraphContext) -> Option<GraphVerdict> {
	let named = graph.named_packages(job);

	if named.is_empty() {
		return None;
	}

	let affected: Vec<String> = named
		.into_iter()
		.filter(|id| graph.affected_package_ids.contains(id))
		.collect();

	if affected.is_empty() {
		return Some(skip_verdict(format!(
			"package-scoped job `{job}` was not affected by the changed files"
		)));
	}

	Some(run_verdict(
		format!(
			"package-scoped job `{job}` affects package(s): {}",
			affected.join(", ")
		),
		affected,
	))
}

/// Run whenever the graph found any affected package.
fn affected_packages_verdict(graph: &WorkspaceGraphContext) -> Option<GraphVerdict> {
	if graph.affected_package_ids.is_empty() {
		return None;
	}

	let affected: Vec<String> = graph.affected_package_ids.iter().cloned().collect();

	Some(run_verdict(
		format!(
			"workspace graph detected {} affected package(s): {}",
			affected.len(),
			affected.join(", ")
		),
		affected,
	))
}

/// Run conservatively when nothing else matched.
fn conservative_verdict(job: &str) -> GraphVerdict {
	run_verdict(
		format!("non-documentation files changed; running `{job}` conservatively"),
		Vec::new(),
	)
}

/// Build a "run" verdict.
fn run_verdict(reason: String, packages: Vec<String>) -> GraphVerdict {
	(true, reason, packages)
}

/// Build a "skip" verdict with no affected packages.
fn skip_verdict(reason: String) -> GraphVerdict {
	(false, reason, Vec::new())
}

impl WorkspaceGraphContext {
	/// Resolve a requested package name or id to its configuration id.
	fn resolve_package_id(&self, requested: &str) -> String {
		self.package_names_to_ids
			.get(requested)
			.cloned()
			.unwrap_or_else(|| requested.to_string())
	}

	/// Package ids whose name appears inside the job name.
	fn named_packages(&self, job: &str) -> Vec<String> {
		let mut packages: Vec<String> = self
			.package_names_to_ids
			.iter()
			.filter(|(name, id)| job_names_package(job, name, id))
			.map(|(_, id)| id.clone())
			.collect();

		packages.sort();
		packages.dedup();

		packages
	}
}

/// Test whether a job name directly references a package.
fn job_names_package(job: &str, name: &str, id: &str) -> bool {
	job == name
		|| job == id
		|| job.starts_with(&format!("{name}:"))
		|| job.ends_with(&format!(":{name}"))
		|| job.starts_with(&format!("{id}:"))
		|| job.ends_with(&format!(":{id}"))
}

/// Detect whether a path is documentation or repository metadata.
pub(super) fn is_doc_or_metadata_path(path: &str) -> bool {
	let lower = path.to_ascii_lowercase();
	let has_doc_extension = Path::new(path)
		.extension()
		.and_then(|extension| extension.to_str())
		.is_some_and(is_doc_extension);

	has_doc_extension
		|| DOC_PATH_PREFIXES
			.iter()
			.any(|prefix| lower.starts_with(prefix))
		|| DOC_FILE_NAMES.contains(&lower.as_str())
}

/// Test whether an extension belongs to the documentation set.
fn is_doc_extension(extension: &str) -> bool {
	DOC_EXTENSIONS
		.iter()
		.any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

/// Detect whether a job builds Rust, which gets a longer duration budget.
pub(super) fn is_rust_job_or_context(job: &str, changed_files: &[String], root: &Path) -> bool {
	let lower = job.to_ascii_lowercase();

	if RUST_JOB_MARKERS.iter().any(|marker| lower.contains(marker)) {
		return true;
	}

	if changed_files.iter().any(|file| is_rust_path(file)) {
		return true;
	}

	root.join("Cargo.toml").is_file() && lower.contains("test")
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
		return Vec::new();
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
fn diff_specification(base: Option<&str>, head: Option<&str>) -> String {
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
		.find(|pattern| glob_matches(value, pattern))
		.cloned()
}

/// Test one glob pattern, falling back to literal equality.
fn glob_matches(value: &str, pattern: &str) -> bool {
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
