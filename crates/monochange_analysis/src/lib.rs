#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![forbid(clippy::indexing_slicing)]
#![doc = include_str!("crate_docs.md")]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Mutex;

#[cfg(feature = "cargo")]
use monochange_cargo::semantic_analyzer_with_settings as cargo_semantic_analyzer;
use monochange_config::apply_version_groups;
use monochange_config::load_workspace_configuration;
use monochange_core::CargoSemverChecksSettings;
use monochange_core::Ecosystem;
use monochange_core::EffectiveReleaseIdentity;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackagePathMatch;
use monochange_core::PackagePathMatcher;
use monochange_core::PackageRecord;
use monochange_core::SemanticAnalyzer;
use monochange_core::git;
use monochange_core::normalize_path;
use monochange_core::relative_to_root;
#[cfg(feature = "dart")]
use monochange_dart::semantic_analyzer as dart_semantic_analyzer;
#[cfg(feature = "deno")]
use monochange_deno::semantic_analyzer as deno_semantic_analyzer;
#[cfg(feature = "npm")]
use monochange_npm::semantic_analyzer as npm_semantic_analyzer;
use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;

const MAX_DISCOVERY_FILE_SIZE: usize = 4 * 1024 * 1024;
const MAX_SNAPSHOT_FILE_SIZE: usize = 256 * 1024;

pub mod frame;

pub use frame::ChangeFrame;
pub use frame::FrameError;
pub use frame::PrEnvironment;
pub use monochange_core::AnalyzedFileChange;
pub use monochange_core::DetectionLevel;
pub use monochange_core::FileChangeKind;
pub use monochange_core::PackageSnapshot;
pub use monochange_core::PackageSnapshotFile;
pub use monochange_core::SemanticAnalysisCompleteness;
pub use monochange_core::SemanticAnalysisOutcome;
pub use monochange_core::SemanticAnalyzerEvidence;
pub use monochange_core::SemanticChange;
pub use monochange_core::SemanticChangeAssessment;
pub use monochange_core::SemanticChangeCategory;
pub use monochange_core::SemanticChangeKind;

/// Placeholder grouping configuration reserved for future lifecycle tooling.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GroupingThresholds {
	/// Maximum number of semantic changes to surface before callers may prefer summarization.
	pub max_detailed_changes: usize,
}

impl Default for GroupingThresholds {
	fn default() -> Self {
		Self {
			max_detailed_changes: 50,
		}
	}
}

/// Configuration for semantic change analysis.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnalysisConfig {
	/// Requested detail level.
	pub detection_level: DetectionLevel,
	/// Reserved for future changeset-grouping helpers.
	pub thresholds: GroupingThresholds,
	/// Maximum number of package summaries a downstream caller wants to post-process.
	pub max_suggestions: usize,
}

impl Default for AnalysisConfig {
	fn default() -> Self {
		Self {
			detection_level: DetectionLevel::Signature,
			thresholds: GroupingThresholds::default(),
			max_suggestions: 10,
		}
	}
}

/// Semantic analysis for one package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackageChangeAnalysis {
	/// Preferred package id for display, using the configured package id when available.
	pub package_id: String,
	/// Underlying discovered package record id.
	pub package_record_id: String,
	/// Package manifest name.
	pub package_name: String,
	/// Package ecosystem.
	pub ecosystem: Ecosystem,
	/// Release identity resolved from the comparison endpoints.
	#[serde(default, skip_serializing)]
	pub release_identity: Option<EffectiveReleaseIdentity>,
	/// Analyzer id that produced the semantic diff.
	pub analyzer_id: Option<String>,
	/// Package-relative changed files.
	pub changed_files: Vec<PathBuf>,
	/// Structured semantic diffs.
	pub semantic_changes: Vec<SemanticChange>,
	/// Non-fatal package analysis warnings.
	pub warnings: Vec<String>,
}

/// Complete semantic analysis for the requested frame.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChangeAnalysis {
	/// Frame analyzed.
	pub frame: ChangeFrame,
	/// Requested detail level.
	pub detection_level: DetectionLevel,
	/// Discovered workspace packages used for dependency propagation.
	#[serde(default, skip_serializing)]
	pub packages: Vec<PackageRecord>,
	/// Semantic diffs grouped by package id.
	pub package_analyses: BTreeMap<String, PackageChangeAnalysis>,
	/// Root-level warnings such as unmatched paths or missing analyzers.
	pub warnings: Vec<String>,
}

/// Explicit refs used for release-aware multi-frame semantic analysis.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReleaseTrajectoryRefs {
	/// Release baseline ref, usually a workspace tag.
	pub release_ref: String,
	/// Default branch ref representing current `main`.
	pub main_ref: String,
	/// Head ref representing the current branch or PR head.
	pub head_ref: String,
}

/// Release-aware semantic analyses for three comparison frames.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReleaseTrajectoryFrames {
	/// Semantic analysis between the last release baseline and current `main`.
	pub release_to_main: ChangeAnalysis,
	/// Semantic analysis between current `main` and the current head/branch.
	pub main_to_head: ChangeAnalysis,
	/// Semantic analysis between the last release baseline and the current head/branch.
	pub release_to_head: ChangeAnalysis,
}

/// Raw multi-frame semantic evidence for release-aware compatibility reasoning.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReleaseTrajectoryAnalysis {
	/// Resolved refs used for the three comparison frames.
	pub refs: ReleaseTrajectoryRefs,
	/// Per-frame semantic analyses.
	pub frames: ReleaseTrajectoryFrames,
	/// Root-level warnings about baseline resolution.
	pub warnings: Vec<String>,
}

#[derive(Default)]
struct AnalyzerRegistry {
	analyzers: Vec<Box<dyn SemanticAnalyzer>>,
}

impl AnalyzerRegistry {
	fn new(cargo_semver_checks: CargoSemverChecksSettings) -> Self {
		let analyzers: Vec<Box<dyn SemanticAnalyzer>> = vec![
			#[cfg(feature = "cargo")]
			Box::new(cargo_semantic_analyzer(cargo_semver_checks)),
			#[cfg(feature = "npm")]
			Box::new(npm_semantic_analyzer()),
			#[cfg(feature = "deno")]
			Box::new(deno_semantic_analyzer()),
			#[cfg(feature = "dart")]
			Box::new(dart_semantic_analyzer()),
		];

		Self { analyzers }
	}

	fn analyzer_for(&self, package: &PackageRecord) -> Option<&dyn SemanticAnalyzer> {
		self.analyzers
			.iter()
			.find(|analyzer| analyzer.applies_to(package))
			.map(AsRef::as_ref)
	}
}

#[derive(Debug, Clone)]
struct SnapshotTargets {
	before: SnapshotTarget,
	after: SnapshotTarget,
}

#[derive(Debug, Clone)]
enum SnapshotTarget {
	GitRevision(String),
	WorkingTree,
	GitIndex,
}

#[derive(Debug, Clone)]
struct AnalysisWorkspace {
	cargo_semver_checks: CargoSemverChecksSettings,
	packages: Vec<PackageRecord>,
	release_identities: BTreeMap<String, EffectiveReleaseIdentity>,
	path_matchers: BTreeMap<String, PackagePathMatcher>,
	workspace_path_matchers: BTreeMap<String, PackagePathMatcher>,
	workspace_path_matcher: PackagePathMatcher,
	warnings: Vec<String>,
}

/// Reusable package discovery and analyzer state for several change frames.
#[non_exhaustive]
pub struct AnalysisSession {
	repo_root: PathBuf,
	workspace: AnalysisWorkspace,
	workspace_cache: Mutex<BTreeMap<String, AnalysisWorkspace>>,
	registry: AnalyzerRegistry,
	config: AnalysisConfig,
}

impl AnalysisSession {
	/// Discover the working workspace and cache snapshot discovery for subsequent frame analyses.
	///
	/// # Errors
	///
	/// Returns an error if workspace configuration or package discovery fails.
	pub fn new(repo_root: &Path, config: AnalysisConfig) -> MonochangeResult<Self> {
		let repo_root = normalize_path(repo_root);
		let workspace = discover_analysis_workspace(&repo_root)?;
		let registry = AnalyzerRegistry::new(workspace.cargo_semver_checks.clone());

		Ok(Self {
			repo_root,
			workspace,
			workspace_cache: Mutex::new(BTreeMap::new()),
			registry,
			config,
		})
	}

	/// Analyze one frame and reuse discovery for Git trees seen by this session.
	///
	/// # Errors
	///
	/// Returns an error if git inspection, snapshot capture, or an analyzer fails.
	pub fn analyze(&self, frame: &ChangeFrame) -> MonochangeResult<ChangeAnalysis> {
		analyze_changes_in_session(self, frame)
	}

	fn workspace_for_target(&self, target: &SnapshotTarget) -> MonochangeResult<AnalysisWorkspace> {
		match target {
			SnapshotTarget::WorkingTree => Ok(self.workspace.clone()),
			SnapshotTarget::GitRevision(revision) => {
				let tree = git_tree_object(&self.repo_root, revision)?;
				let mut cache = self.workspace_cache.lock().map_err(|_| {
					MonochangeError::Discovery(
						"package-discovery workspace cache was poisoned".to_string(),
					)
				})?;
				if let Some(workspace) = cache.get(&tree) {
					return Ok(workspace.clone());
				}
				let workspace = discover_analysis_workspace_from_snapshot(
					&self.repo_root,
					&SnapshotTarget::GitRevision(tree.clone()),
					// patch-coverage:ignore-start -- snapshot discovery is exercised; Linux llvm-cov attributes the closing `?` separately.
				)?;
				// patch-coverage:ignore-end
				cache.insert(tree, workspace.clone());
				Ok(workspace)
			}
			SnapshotTarget::GitIndex => {
				discover_analysis_workspace_from_snapshot(&self.repo_root, target)
			}
		}
	}
}

fn git_tree_object(repo_root: &Path, revision: &str) -> MonochangeResult<String> {
	let object = format!("{revision}^{{tree}}");
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["rev-parse", "--verify", &object])
		.output()
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to resolve git tree `{object}`: {error}"))
		})?;
	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"failed to resolve git tree `{object}`: {}",
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}
	String::from_utf8(output.stdout)
		.map(|value| value.trim().to_string())
		// patch-coverage:ignore-start -- `git rev-parse` emits an ASCII object id; invalid output requires replacing Git itself.
		.map_err(|error| {
			MonochangeError::Discovery(format!("git tree `{object}` is not valid utf-8: {error}"))
		})
	// patch-coverage:ignore-end
}

/// Analyze changes for the requested frame.
///
/// # Errors
///
/// Returns an error if workspace discovery, git inspection, or analyzer
/// execution fails.
pub fn analyze_changes(
	repo_root: &Path,
	frame: &ChangeFrame,
	config: &AnalysisConfig,
) -> MonochangeResult<ChangeAnalysis> {
	AnalysisSession::new(repo_root, config.clone())?.analyze(frame)
}

fn analyze_changes_in_session(
	session: &AnalysisSession,
	frame: &ChangeFrame,
) -> MonochangeResult<ChangeAnalysis> {
	let changed_paths = frame
		.changed_files(&session.repo_root)?
		.into_iter()
		.collect::<Vec<_>>();
	let targets = resolve_snapshot_targets(&session.repo_root, frame)?;
	let before_workspace = session.workspace_for_target(&targets.before)?;
	let after_workspace = session.workspace_for_target(&targets.after)?;
	let workspace = merge_analysis_workspaces(before_workspace, after_workspace);
	let changed_paths = changed_paths
		.into_iter()
		.filter(|path| workspace.workspace_path_matcher.classify(path) != PackagePathMatch::Ignored)
		.collect::<Vec<_>>();
	let mut warnings = workspace.warnings.clone();
	let mut package_analyses = BTreeMap::new();
	// patch-coverage:ignore-start -- success and failure are covered through analysis sessions; llvm-cov attributes the closing `?` expression inconsistently.
	let package_inputs = package_inputs(
		&session.repo_root,
		&workspace.packages,
		&workspace.path_matchers,
		&workspace.workspace_path_matchers,
		&changed_paths,
		&targets,
	)?;
	// patch-coverage:ignore-end
	let matched_paths = package_inputs
		.values()
		.flat_map(|value| value.iter().map(|change| change.path.clone()))
		.collect::<BTreeSet<_>>();

	for path in changed_paths {
		if !matched_paths.contains(&path) {
			warnings.push(format!(
				"changed path `{}` did not match any configured package",
				path.display()
			));
		}
	}

	for package in &workspace.packages {
		let Some(changed_files) = package_inputs.get(&package.id) else {
			continue;
		};

		let package_root = package_root_relative(&session.repo_root, package)
			.expect("analyzed packages should have a repository-relative root");
		let package_matcher = workspace.path_matchers.get(&package.id);
		let workspace_matcher = workspace
			.workspace_path_matchers
			.get(&package.id)
			.unwrap_or(&workspace.workspace_path_matcher);
		let mut before_snapshot = snapshot_package(&session.repo_root, package, &targets.before)?;
		let mut after_snapshot = snapshot_package(&session.repo_root, package, &targets.after)?;
		let lifecycle_change = package_lifecycle_change(package, &before_snapshot, &after_snapshot);
		filter_snapshot_files(
			&mut before_snapshot,
			&package_root,
			workspace_matcher,
			package_matcher,
		);
		filter_snapshot_files(
			&mut after_snapshot,
			&package_root,
			workspace_matcher,
			package_matcher,
		);
		let package_id = preferred_package_id(package);
		let package_changed_files = changed_files
			.iter()
			.map(|file| file.package_path.clone())
			.collect::<Vec<_>>();

		let analyzer = session.registry.analyzer_for(package).expect(
			"semantic analyzer registry should cover all discovered ecosystems when all default features are enabled",
		);

		let context = PackageAnalysisContext {
			repo_root: &session.repo_root,
			package,
			detection_level: session.config.detection_level,
			changed_files,
			before_snapshot: Some(&before_snapshot),
			after_snapshot: Some(&after_snapshot),
		};
		let mut result = analyzer.analyze_package(&context)?;
		if let Some(lifecycle_change) = lifecycle_change {
			result.semantic_changes.push(lifecycle_change);
		}
		let release_identity = workspace.release_identities.get(&package_id).cloned();
		package_analyses.insert(
			package_id.clone(),
			PackageChangeAnalysis {
				package_id,
				package_record_id: package.id.clone(),
				package_name: package.name.clone(),
				ecosystem: package.ecosystem,
				release_identity,
				analyzer_id: Some(result.analyzer_id),
				changed_files: package_changed_files,
				semantic_changes: result.semantic_changes,
				warnings: result.warnings,
			},
		);
	}

	Ok(ChangeAnalysis {
		frame: frame.clone(),
		detection_level: session.config.detection_level,
		packages: workspace.packages,
		package_analyses,
		warnings,
	})
}

fn package_lifecycle_change(
	package: &PackageRecord,
	before: &PackageSnapshot,
	after: &PackageSnapshot,
) -> Option<SemanticChange> {
	let manifest_path = package.manifest_path.file_name().map(PathBuf::from)?;
	let before_has_manifest = before.files.iter().any(|file| file.path == manifest_path);
	let after_has_manifest = after.files.iter().any(|file| file.path == manifest_path);
	let kind = match (before_has_manifest, after_has_manifest) {
		(true, false) => SemanticChangeKind::Removed,
		(false, true) => SemanticChangeKind::Added,
		_ => return None,
	};
	let descriptor = format!("{} package `{}`", package.ecosystem, package.name);
	let summary = match kind {
		SemanticChangeKind::Added => format!("added {descriptor}"),
		SemanticChangeKind::Removed => format!("removed {descriptor}"),
		// patch-coverage:ignore-start -- the match above constructs only added or removed lifecycle changes.
		_ => return None,
		// patch-coverage:ignore-end
	};

	let mut change = SemanticChange::new(
		SemanticChangeCategory::Package,
		kind,
		"package",
		package.name.clone(),
		summary,
		manifest_path,
	);
	change.before_signature = before_has_manifest.then(|| descriptor.clone());
	change.after_signature = after_has_manifest.then_some(descriptor);
	Some(change)
}

/// Analyze three release-aware semantic frames using explicit refs.
///
/// # Errors
///
/// Returns an error if any frame analysis or ref resolution fails.
pub fn analyze_release_trajectory_for_refs(
	repo_root: &Path,
	refs: &ReleaseTrajectoryRefs,
	config: &AnalysisConfig,
) -> MonochangeResult<ReleaseTrajectoryAnalysis> {
	let release_to_main =
		analyze_custom_range(repo_root, &refs.release_ref, &refs.main_ref, config)?;
	let main_to_head = analyze_custom_range(repo_root, &refs.main_ref, &refs.head_ref, config)?;
	let release_to_head =
		analyze_custom_range(repo_root, &refs.release_ref, &refs.head_ref, config)?;
	let mut warnings = Vec::new();

	if refs.main_ref == refs.head_ref {
		warnings.push(
			"release trajectory head matches main; `main_to_head` only reflects changes currently on the default branch"
				.to_string(),
		);
	}

	Ok(ReleaseTrajectoryAnalysis {
		refs: refs.clone(),
		frames: ReleaseTrajectoryFrames {
			release_to_main,
			main_to_head,
			release_to_head,
		},
		warnings,
	})
}

/// Analyze three release-aware semantic frames using the latest workspace tag,
/// the detected default branch, and the current branch.
///
/// # Errors
///
/// Returns an error if release-baseline or branch resolution fails.
pub async fn analyze_release_trajectory(
	repo_root: &Path,
	config: &AnalysisConfig,
) -> MonochangeResult<ReleaseTrajectoryAnalysis> {
	let repo_root = normalize_path(repo_root);
	let refs = ReleaseTrajectoryRefs {
		release_ref: latest_workspace_release_tag(&repo_root).await?,
		main_ref: default_branch_ref(&repo_root)?,
		head_ref: git::git_current_branch(&repo_root).await?,
	};

	analyze_release_trajectory_for_refs(&repo_root, &refs, config)
}

fn analyze_custom_range(
	repo_root: &Path,
	base: &str,
	head: &str,
	config: &AnalysisConfig,
) -> MonochangeResult<ChangeAnalysis> {
	analyze_changes(
		repo_root,
		&ChangeFrame::CustomRange {
			base: base.to_string(),
			head: head.to_string(),
		},
		config,
	)
}

fn preferred_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

async fn latest_workspace_release_tag(repo_root: &Path) -> MonochangeResult<String> {
	let output = git::git_command_output(repo_root, &["tag", "--list", "--sort=-v:refname"])
		.await
		.map_err(|error| MonochangeError::Io(format!("failed to list git tags: {error}")))?;

	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to list git tags: {}",
			git::git_error_detail(&output)
		)));
	}

	String::from_utf8_lossy(&output.stdout)
		.lines()
		.map(str::trim)
		.find(|tag| tag.starts_with('v') && !tag.contains('/'))
		.map(ToString::to_string)
		.ok_or_else(|| {
			MonochangeError::Config(
				"failed to resolve a workspace release baseline from git tags".to_string(),
			)
		})
}

fn default_branch_ref(repo_root: &Path) -> MonochangeResult<String> {
	frame::default_branch_name(repo_root).map_err(Into::into)
}

fn discover_analysis_workspace(root: &Path) -> MonochangeResult<AnalysisWorkspace> {
	let configuration = load_workspace_configuration(root)?;
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	#[cfg(feature = "cargo")]
	{
		let discovery = monochange_cargo::discover_cargo_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	#[cfg(feature = "npm")]
	{
		let discovery = monochange_npm::discover_npm_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	#[cfg(feature = "deno")]
	{
		let discovery = monochange_deno::discover_deno_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	#[cfg(feature = "dart")]
	{
		let discovery = monochange_dart::discover_dart_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	let (_, version_group_warnings) = apply_version_groups(&mut packages, &configuration)?;
	warnings.extend(version_group_warnings);
	if !configuration.packages.is_empty() {
		packages.retain(|package| package.metadata.contains_key("config_id"));
	} // patch-coverage:ignore-start patch-coverage:ignore-end -- configured-package filtering is exercised by every configured analysis fixture; llvm-cov attributes the closing brace inconsistently.
	let workspace_path_matcher = PackagePathMatcher::new(
		"workspace",
		Path::new(""),
		&[],
		&configuration.changesets.affected.ignored_paths,
	);
	let workspace_path_matchers = packages
		.iter()
		.map(|package| (package.id.clone(), workspace_path_matcher.clone()))
		.collect();
	let path_matchers = packages
		.iter()
		.filter_map(|package| {
			let config_id = package.metadata.get("config_id")?;
			let definition = configuration.package_by_id(config_id)?;
			Some((
				package.id.clone(),
				PackagePathMatcher::new(
					config_id,
					&definition.path,
					&definition.additional_paths,
					&definition.ignored_paths,
				),
			))
		})
		.collect();
	let release_identities = packages
		.iter()
		.filter_map(|package| {
			let package_id = preferred_package_id(package);
			configuration
				.effective_release_identity(&package_id)
				.map(|identity| (package_id, identity))
		})
		.collect();

	Ok(AnalysisWorkspace {
		cargo_semver_checks: configuration.cargo.semver_checks.clone(),
		packages,
		release_identities,
		path_matchers,
		workspace_path_matchers,
		workspace_path_matcher,
		warnings,
	})
}

fn discover_analysis_workspace_from_snapshot(
	repo_root: &Path,
	target: &SnapshotTarget,
) -> MonochangeResult<AnalysisWorkspace> {
	// patch-coverage:ignore-start -- temporary-directory allocation failures depend on host-level I/O exhaustion or permissions.
	let snapshot = tempfile::tempdir().map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create a temporary workspace for package discovery: {error}"
		))
	})?;
	// patch-coverage:ignore-end
	materialize_discovery_files(repo_root, target, snapshot.path())?;
	let mut workspace = discover_analysis_workspace(snapshot.path())?;
	rebase_discovered_workspace(&mut workspace, snapshot.path(), repo_root)?;
	Ok(workspace)
}

fn materialize_discovery_files(
	repo_root: &Path,
	target: &SnapshotTarget,
	destination: &Path,
) -> MonochangeResult<()> {
	let paths = match target {
		SnapshotTarget::GitRevision(revision) => {
			git_list_files(repo_root, &["ls-tree", "-r", "--name-only", revision])?
		}
		SnapshotTarget::GitIndex => git_list_files(repo_root, &["ls-files", "--cached"])?,
		SnapshotTarget::WorkingTree => {
			return Err(MonochangeError::Discovery(
				"working-tree discovery should use the existing workspace".to_string(),
			));
		}
	};

	let paths = paths
		.into_iter()
		.filter(|path| is_discovery_file(path))
		.collect::<Vec<_>>();
	validate_discovery_paths(&paths)?;
	let files = match target {
		SnapshotTarget::GitRevision(revision) => {
			build_revision_snapshot_files_with_limit(
				repo_root,
				Path::new(""),
				revision,
				&paths,
				MAX_DISCOVERY_FILE_SIZE,
				// patch-coverage:ignore-start -- the revision branch is exercised; llvm-cov attributes the closing `?` separately.
			)?
			// patch-coverage:ignore-end
		}
		SnapshotTarget::GitIndex => {
			// patch-coverage:ignore-start -- index materialization is exercised; llvm-cov leaves a zero-count subregion within the argument list.
			build_snapshot_files_from_paths_with_limit(
				repo_root,
				Path::new(""),
				target,
				&paths,
				MAX_DISCOVERY_FILE_SIZE,
			)?
			// patch-coverage:ignore-end
		}
		// patch-coverage:ignore-start -- working-tree discovery returns before this exhaustive-match safeguard.
		SnapshotTarget::WorkingTree => unreachable!("working-tree discovery returned above"),
		// patch-coverage:ignore-end
	};
	let materialized_paths = files
		.iter()
		.map(|file| file.path.clone())
		.collect::<BTreeSet<_>>();
	for file in files {
		write_discovery_file(destination, &file.path, file.contents.as_bytes())?;
	}
	for path in paths {
		if materialized_paths.contains(&path) {
			continue;
		}
		if path.file_name().is_some_and(|name| name == "bun.lockb") {
			write_discovery_file(destination, &path, &[])?;
			continue;
		}
		return Err(MonochangeError::Discovery(format!(
			"snapshot discovery file `{}` is not valid utf-8 or exceeds {} bytes",
			path.display(),
			MAX_DISCOVERY_FILE_SIZE
		)));
	}

	Ok(())
}

fn validate_discovery_paths(paths: &[PathBuf]) -> MonochangeResult<()> {
	for path in paths {
		if !is_safe_repository_path(path) {
			return Err(MonochangeError::Discovery(format!(
				"refused to materialize unsafe repository path `{}`",
				path.display()
			)));
		}
	}

	Ok(())
}

fn write_discovery_file(destination: &Path, path: &Path, contents: &[u8]) -> MonochangeResult<()> {
	let output_path = destination.join(path);
	let parent = output_path.parent().unwrap_or(destination);
	fs::create_dir_all(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create snapshot directory {}: {error}",
			parent.display()
		))
	})?;
	fs::write(&output_path, contents).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write snapshot file {}: {error}",
			output_path.display()
		))
	})
}

fn is_discovery_file(path: &Path) -> bool {
	path.file_name()
		.and_then(|name| name.to_str())
		.is_some_and(|name| {
			matches!(
				name,
				".gitignore"
					| "Cargo.toml" | "bun.lockb"
					| "deno.json" | "deno.jsonc"
					| "monochange.toml"
					| "package.json"
					| "pnpm-workspace.yaml"
					| "pubspec.yaml"
			)
		})
}

fn is_safe_repository_path(path: &Path) -> bool {
	!path.as_os_str().is_empty()
		&& path.components().all(|component| {
			matches!(
				component,
				std::path::Component::Normal(_) | std::path::Component::CurDir
			)
		})
}

fn rebase_discovered_workspace(
	workspace: &mut AnalysisWorkspace,
	snapshot_root: &Path,
	repo_root: &Path,
) -> MonochangeResult<()> {
	for package in &mut workspace.packages {
		let manifest =
			relative_to_root(snapshot_root, &package.manifest_path).ok_or_else(|| {
				MonochangeError::Discovery(format!(
					"snapshot package manifest {} is outside {}",
					package.manifest_path.display(),
					snapshot_root.display()
				))
			})?;
		let package_workspace = relative_to_root(snapshot_root, &package.workspace_root)
			.ok_or_else(|| {
				MonochangeError::Discovery(format!(
					"snapshot package workspace {} is outside {}",
					package.workspace_root.display(),
					snapshot_root.display()
				))
			})?;
		package.manifest_path = repo_root.join(manifest);
		package.workspace_root = repo_root.join(package_workspace);
	}
	normalize_package_ids(repo_root, &mut workspace.packages);

	let snapshot_display = snapshot_root.display().to_string();
	let repo_display = repo_root.display().to_string();
	for warning in &mut workspace.warnings {
		*warning = warning.replace(&snapshot_display, &repo_display);
	}

	Ok(())
}

fn merge_analysis_workspaces(
	before: AnalysisWorkspace,
	after: AnalysisWorkspace,
) -> AnalysisWorkspace {
	let mut packages = before
		.packages
		.into_iter()
		.map(|package| (package.id.clone(), package))
		.collect::<BTreeMap<_, _>>();
	packages.extend(
		after
			.packages
			.into_iter()
			.map(|package| (package.id.clone(), package)),
	);
	let mut path_matchers = before.path_matchers;
	path_matchers.extend(after.path_matchers);
	let mut workspace_path_matchers = before.workspace_path_matchers;
	workspace_path_matchers.extend(after.workspace_path_matchers);
	let mut release_identities = before.release_identities;
	release_identities.extend(after.release_identities);
	let mut warnings = before.warnings;
	warnings.extend(after.warnings);
	warnings.sort();
	warnings.dedup();

	AnalysisWorkspace {
		cargo_semver_checks: after.cargo_semver_checks,
		packages: packages.into_values().collect(),
		release_identities,
		path_matchers,
		workspace_path_matchers,
		workspace_path_matcher: after.workspace_path_matcher,
		warnings,
	}
}

fn normalize_package_ids(root: &Path, packages: &mut [PackageRecord]) {
	for package in packages {
		let Some(relative_manifest) = relative_to_root(root, &package.manifest_path) else {
			continue;
		};
		package.id = format!(
			"{}:{}",
			package.ecosystem.as_str(),
			relative_manifest.display()
		);
	}
}

fn package_inputs(
	repo_root: &Path,
	packages: &[PackageRecord],
	path_matchers: &BTreeMap<String, PackagePathMatcher>,
	workspace_path_matchers: &BTreeMap<String, PackagePathMatcher>,
	changed_paths: &[PathBuf],
	targets: &SnapshotTargets,
) -> MonochangeResult<BTreeMap<String, Vec<AnalyzedFileChange>>> {
	let mut inputs = BTreeMap::<String, Vec<AnalyzedFileChange>>::new();

	for changed_path in changed_paths {
		let package_matches = packages_for_path(
			repo_root,
			packages,
			path_matchers,
			workspace_path_matchers,
			changed_path,
		);
		for package in package_matches {
			let package_root = package_root_relative(repo_root, package)
				.expect("package path matching should only return packages with a resolvable root");
			let package_path = changed_path
				.strip_prefix(&package_root)
				.map_or_else(|_| changed_path.clone(), Path::to_path_buf);
			let before_contents =
				read_text_file_from_target(repo_root, &targets.before, changed_path)?;
			let after_contents =
				read_text_file_from_target(repo_root, &targets.after, changed_path)?;
			let kind = classify_file_change(before_contents.as_ref(), after_contents.as_ref());
			inputs
				.entry(package.id.clone())
				.or_default()
				.push(AnalyzedFileChange {
					path: changed_path.clone(),
					package_path,
					kind,
					before_contents,
					after_contents,
				});
		}
	}

	for changes in inputs.values_mut() {
		changes.sort_by(|left, right| left.package_path.cmp(&right.package_path));
	}

	Ok(inputs)
}

fn packages_for_path<'a>(
	repo_root: &Path,
	packages: &'a [PackageRecord],
	path_matchers: &BTreeMap<String, PackagePathMatcher>,
	workspace_path_matchers: &BTreeMap<String, PackagePathMatcher>,
	changed_path: &Path,
) -> Vec<&'a PackageRecord> {
	let configured_matches = packages
		.iter()
		.filter(|package| {
			if workspace_path_matchers
				.get(&package.id)
				.is_some_and(|matcher| matcher.classify(changed_path) == PackagePathMatch::Ignored)
			{
				return false;
			}
			path_matchers
				.get(&package.id)
				.is_some_and(|matcher| matcher.classify(changed_path) == PackagePathMatch::Touched)
		})
		.collect::<Vec<_>>();

	if !configured_matches.is_empty() {
		return configured_matches;
	}

	let mut matches = packages
		.iter()
		.filter(|package| !path_matchers.contains_key(&package.id))
		.filter(|package| {
			!workspace_path_matchers
				.get(&package.id)
				.is_some_and(|matcher| matcher.classify(changed_path) == PackagePathMatch::Ignored)
		})
		.filter_map(|package| {
			let package_root = package_root_relative(repo_root, package)?;
			(changed_path == package_root || changed_path.starts_with(&package_root))
				.then_some((package_root.components().count(), package))
		})
		.collect::<Vec<_>>();

	let Some(longest_match) = matches.iter().map(|(depth, _)| *depth).max() else {
		return Vec::new();
	};

	matches.retain(|(depth, _)| *depth == longest_match);
	matches.into_iter().map(|(_, package)| package).collect()
}

fn package_root_relative(repo_root: &Path, package: &PackageRecord) -> Option<PathBuf> {
	let package_root = package
		.manifest_path
		.parent()
		.unwrap_or(&package.workspace_root);
	relative_to_root(repo_root, package_root)
}

fn filter_snapshot_files(
	snapshot: &mut PackageSnapshot,
	package_root: &Path,
	workspace_matcher: &PackagePathMatcher,
	package_matcher: Option<&PackagePathMatcher>,
) {
	snapshot.files.retain(|file| {
		let repository_path = package_root.join(&file.path);
		workspace_matcher.classify(&repository_path) != PackagePathMatch::Ignored
			&& !package_matcher.is_some_and(|matcher| {
				matcher.classify(&repository_path) == PackagePathMatch::Ignored
			})
	});
}

fn classify_file_change(before: Option<&String>, after: Option<&String>) -> FileChangeKind {
	match (before, after) {
		(None, Some(_)) => FileChangeKind::Added,
		(Some(_), None) => FileChangeKind::Deleted,
		_ => FileChangeKind::Modified,
	}
}

fn resolve_snapshot_targets(
	repo_root: &Path,
	frame: &ChangeFrame,
) -> Result<SnapshotTargets, FrameError> {
	match frame {
		ChangeFrame::WorkingDirectory => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision("HEAD".to_string()),
				after: SnapshotTarget::WorkingTree,
			})
		}
		ChangeFrame::StagedOnly => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision("HEAD".to_string()),
				after: SnapshotTarget::GitIndex,
			})
		}
		ChangeFrame::BranchRange { base, head } | ChangeFrame::CustomRange { base, head } => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision(base.clone()),
				after: SnapshotTarget::GitRevision(head.clone()),
			})
		}
		ChangeFrame::PullRequest { target, pr_branch } => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision(git_merge_base(repo_root, target, pr_branch)?),
				after: SnapshotTarget::GitRevision(pr_branch.clone()),
			})
		}
	}
}

fn git_merge_base(repo_root: &Path, base: &str, head: &str) -> Result<String, FrameError> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["merge-base", base, head])
		.output()
		.map_err(|error| FrameError::Git(format!("failed to run git merge-base: {error}")))?;

	if !output.status.success() {
		return Err(FrameError::Git(format!(
			"git merge-base {base} {head} failed"
		)));
	}

	String::from_utf8(output.stdout)
		.map_err(|error| FrameError::Git(format!("invalid utf-8 from git merge-base: {error}")))
		.map(|value| value.trim().to_string())
}

fn snapshot_package(
	repo_root: &Path,
	package: &PackageRecord,
	target: &SnapshotTarget,
) -> MonochangeResult<PackageSnapshot> {
	let package_root = package_root_relative(repo_root, package).ok_or_else(|| {
		MonochangeError::Discovery(format!(
			"failed to resolve package root for `{}`",
			package.id
		))
	})?;
	let label = snapshot_label(repo_root, target)?;
	let files = match target {
		SnapshotTarget::WorkingTree => snapshot_files_from_working_tree(repo_root, &package_root)?,
		SnapshotTarget::GitRevision(revision) => {
			snapshot_files_from_revision(repo_root, &package_root, revision)?
		}
		SnapshotTarget::GitIndex => snapshot_files_from_index(repo_root, &package_root)?,
	};

	Ok(PackageSnapshot { label, files })
}

fn snapshot_label(repo_root: &Path, target: &SnapshotTarget) -> MonochangeResult<String> {
	match target {
		SnapshotTarget::GitRevision(revision) => git_tree_object(repo_root, revision),
		SnapshotTarget::WorkingTree => working_tree_object(repo_root),
		SnapshotTarget::GitIndex => git_index_tree_object(repo_root),
	}
}

fn working_tree_object(repo_root: &Path) -> MonochangeResult<String> {
	// patch-coverage:ignore-start -- exercising temporary-directory allocation failure requires exhausting OS resources.
	let temporary = tempfile::tempdir().map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create a temporary index for semantic analysis: {error}"
		))
	})?;
	// patch-coverage:ignore-end
	let index = temporary.path().join("index");
	git_with_index(repo_root, &index, &["read-tree", "HEAD"])?;
	git_with_index(repo_root, &index, &["add", "--all", "--", "."])?;
	git_with_index(repo_root, &index, &["write-tree"])
}

fn git_index_tree_object(repo_root: &Path) -> MonochangeResult<String> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["write-tree"])
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to run git write-tree: {error}")))?;
	git_object_output(output, "git write-tree")
}

fn git_with_index(repo_root: &Path, index: &Path, args: &[&str]) -> MonochangeResult<String> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.env("GIT_INDEX_FILE", index)
		.args(args)
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to run git {args:?}: {error}")))?;
	git_object_output(output, &format!("git {args:?}"))
}

fn git_object_output(output: std::process::Output, operation: &str) -> MonochangeResult<String> {
	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"{operation} failed while resolving a semantic-analysis snapshot: {}",
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}
	String::from_utf8(output.stdout)
		.map(|value| value.trim().to_string())
		// patch-coverage:ignore-start -- Git emits ASCII object ids; invalid output requires replacing the executable after spawn.
		.map_err(|error| {
			MonochangeError::Discovery(format!(
				"{operation} returned invalid UTF-8 while resolving a semantic-analysis snapshot: {error}"
			))
		})
	// patch-coverage:ignore-end
}

#[allow(clippy::unnecessary_wraps)]
fn snapshot_files_from_working_tree(
	repo_root: &Path,
	package_root: &Path,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let absolute_root = repo_root.join(package_root);
	if !absolute_root.exists() {
		return Ok(Vec::new());
	}

	let mut files = Vec::new();
	for entry in WalkDir::new(&absolute_root)
		.into_iter()
		.filter_map(Result::ok)
	{
		let entry_path = entry.path();
		if entry.file_type().is_dir() && should_skip_directory(entry_path) {
			continue;
		}
		if !entry.file_type().is_file() {
			continue;
		}
		let relative_to_package = entry_path
			.strip_prefix(&absolute_root)
			.unwrap_or(entry_path);
		let Some(contents) = read_working_tree_text(entry_path) else {
			continue;
		};
		files.push(PackageSnapshotFile {
			path: relative_to_package.to_path_buf(),
			contents,
		});
	}

	files.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(files)
}

fn should_skip_directory(path: &Path) -> bool {
	path.file_name().is_some_and(|name| {
		matches!(
			name.to_string_lossy().as_ref(),
			".git" | "target" | "node_modules" | "dist" | "build"
		)
	})
}

fn read_working_tree_text(path: &Path) -> Option<String> {
	let metadata = fs::metadata(path).ok()?;
	(metadata.len() <= 256 * 1024).then_some(())?;
	fs::read_to_string(path).ok()
}

fn snapshot_files_from_revision(
	repo_root: &Path,
	package_root: &Path,
	revision: &str,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let package_root_text = git_package_pathspec(package_root);
	let args = [
		"ls-tree",
		"-r",
		"--name-only",
		revision,
		"--",
		package_root_text.as_str(),
	];
	let paths = git_list_files(repo_root, &args)?;
	build_revision_snapshot_files(repo_root, package_root, revision, &paths)
}

fn build_revision_snapshot_files(
	repo_root: &Path,
	package_root: &Path,
	revision: &str,
	paths: &[PathBuf],
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	build_revision_snapshot_files_with_limit(
		repo_root,
		package_root,
		revision,
		paths,
		MAX_SNAPSHOT_FILE_SIZE,
	)
}

fn build_revision_snapshot_files_with_limit(
	repo_root: &Path,
	package_root: &Path,
	revision: &str,
	paths: &[PathBuf],
	max_file_size: usize,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let mut child = Command::new("git")
		.current_dir(repo_root)
		.args(["cat-file", "--batch"])
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.map_err(|error| MonochangeError::Io(format!("failed to run git cat-file: {error}")))?;
	let mut stdout = child
		.stdout
		.take()
		.ok_or_else(|| MonochangeError::Io("failed to open git cat-file stdout".to_string()))?;
	let output_reader = std::thread::spawn(move || {
		let mut output = Vec::new();
		stdout.read_to_end(&mut output).map(|_| output)
	});
	let mut stderr = child
		.stderr
		.take()
		.ok_or_else(|| MonochangeError::Io("failed to open git cat-file stderr".to_string()))?;
	let error_reader = std::thread::spawn(move || {
		let mut output = Vec::new();
		stderr.read_to_end(&mut output).map(|_| output)
	});
	let mut stdin = child
		.stdin
		.take()
		.ok_or_else(|| MonochangeError::Io("failed to open git cat-file stdin".to_string()))?;
	let write_result = write_revision_snapshot_requests(&mut stdin, revision, paths);
	drop(stdin);

	// patch-coverage:ignore-start -- child wait and reader failures require invalidating handles owned exclusively by this function.
	let status = child.wait().map_err(|error| {
		MonochangeError::Io(format!("failed to wait for git cat-file: {error}"))
	})?;
	let output = output_reader
		.join()
		.map_err(|_| MonochangeError::Io("git cat-file output reader panicked".to_string()))?
		.map_err(|error| {
			MonochangeError::Io(format!("failed to read git cat-file output: {error}"))
		})?;
	let stderr = error_reader
		.join()
		.map_err(|_| MonochangeError::Io("git cat-file error reader panicked".to_string()))?
		.map_err(|error| {
			MonochangeError::Io(format!("failed to read git cat-file errors: {error}"))
		})?;
	// patch-coverage:ignore-end
	if !status.success() {
		return Err(MonochangeError::Discovery(format!(
			"git cat-file failed: {}",
			String::from_utf8_lossy(&stderr).trim()
		)));
	}
	write_result?;

	if max_file_size == MAX_SNAPSHOT_FILE_SIZE {
		parse_revision_snapshot_batch(package_root, paths, &output)
	} else {
		parse_revision_snapshot_batch_with_limit(package_root, paths, &output, max_file_size)
	}
}

fn write_revision_snapshot_requests(
	writer: &mut dyn std::io::Write,
	revision: &str,
	paths: &[PathBuf],
) -> MonochangeResult<()> {
	for path in paths {
		writeln!(writer, "{revision}:{}", path.to_string_lossy()).map_err(|error| {
			MonochangeError::Io(format!("failed to write git cat-file input: {error}"))
		})?;
	}

	Ok(())
}

fn parse_revision_snapshot_batch(
	package_root: &Path,
	paths: &[PathBuf],
	output: &[u8],
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	parse_revision_snapshot_batch_with_limit(package_root, paths, output, MAX_SNAPSHOT_FILE_SIZE)
}

fn parse_revision_snapshot_batch_with_limit(
	package_root: &Path,
	paths: &[PathBuf],
	output: &[u8],
	max_file_size: usize,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let mut cursor = 0;
	let mut files = Vec::new();
	for path in paths {
		// patch-coverage:ignore-start -- cursor advancement is bounded by slices below, so the outer slice and computed header slice cannot be absent.
		let remaining = output.get(cursor..).ok_or_else(|| {
			MonochangeError::Discovery("truncated git cat-file batch output".to_string())
		})?;
		// patch-coverage:ignore-end
		let header_end = remaining
			.iter()
			.position(|byte| *byte == b'\n')
			.map(|position| cursor + position)
			.ok_or_else(|| {
				MonochangeError::Discovery("truncated git cat-file batch header".to_string())
			})?;
		// patch-coverage:ignore-start -- header_end is derived from the same remaining output slice.
		let header = String::from_utf8_lossy(output.get(cursor..header_end).ok_or_else(|| {
			MonochangeError::Discovery("truncated git cat-file batch header".to_string())
		})?);
		// patch-coverage:ignore-end
		cursor = header_end + 1;
		if header.ends_with(" missing") {
			continue;
		}
		let size = header
			.rsplit_once(' ')
			.and_then(|(_, size)| size.parse::<usize>().ok())
			.ok_or_else(|| {
				MonochangeError::Discovery(format!("invalid git cat-file batch header `{header}`"))
			})?;
		let content_end = cursor.checked_add(size).ok_or_else(|| {
			MonochangeError::Discovery("git cat-file batch size overflow".to_string())
		})?;
		let contents = output.get(cursor..content_end).ok_or_else(|| {
			MonochangeError::Discovery("truncated git cat-file batch content".to_string())
		})?;
		// patch-coverage:ignore-start -- content_end must index the output slice, so adding the protocol newline cannot overflow usize.
		cursor = content_end.checked_add(1).ok_or_else(|| {
			MonochangeError::Discovery("git cat-file batch cursor overflow".to_string())
		})?;
		// patch-coverage:ignore-end

		let Some(relative_to_package) = path.strip_prefix(package_root).ok().map(Path::to_path_buf)
		else {
			continue;
		};
		if size > max_file_size {
			continue;
		}
		let Ok(contents) = String::from_utf8(contents.to_vec()) else {
			continue;
		};
		files.push(PackageSnapshotFile {
			path: relative_to_package,
			contents,
		});
	}

	files.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(files)
}

fn snapshot_files_from_index(
	repo_root: &Path,
	package_root: &Path,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let package_root_text = git_package_pathspec(package_root);
	let args = ["ls-files", "--cached", "--", package_root_text.as_str()];
	let paths = git_list_files(repo_root, &args)?;
	build_snapshot_files_from_paths(repo_root, package_root, &SnapshotTarget::GitIndex, &paths)
}

fn git_package_pathspec(package_root: &Path) -> String {
	if package_root.as_os_str().is_empty() {
		".".to_string()
	} else {
		package_root.to_string_lossy().into_owned()
	}
}

fn build_snapshot_files_from_paths(
	repo_root: &Path,
	package_root: &Path,
	target: &SnapshotTarget,
	paths: &[PathBuf],
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	build_snapshot_files_from_paths_with_limit(
		repo_root,
		package_root,
		target,
		paths,
		MAX_SNAPSHOT_FILE_SIZE,
	)
}

fn build_snapshot_files_from_paths_with_limit(
	repo_root: &Path,
	package_root: &Path,
	target: &SnapshotTarget,
	paths: &[PathBuf],
	max_file_size: usize,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let mut files = Vec::new();
	for path in paths {
		let Some(relative_to_package) = path.strip_prefix(package_root).ok().map(Path::to_path_buf)
		else {
			continue;
		};
		let Some(contents) =
			read_text_file_from_target_with_limit(repo_root, target, path, max_file_size)?
		else {
			continue;
		};
		files.push(PackageSnapshotFile {
			path: relative_to_package,
			contents,
		});
	}
	files.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(files)
}

fn git_list_files(repo_root: &Path, args: &[&str]) -> MonochangeResult<Vec<PathBuf>> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(args)
		.output()
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to run git {args:?}: {error}"))
		})?;

	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"git {:?} failed with status {}",
			args, output.status
		)));
	}

	let stdout = String::from_utf8(output.stdout).map_err(|error| {
		MonochangeError::Discovery(format!("git {args:?} returned invalid utf-8: {error}"))
	})?;

	Ok(stdout
		.lines()
		.filter(|line| !line.is_empty())
		.map(PathBuf::from)
		.collect())
}

fn read_text_file_from_target(
	repo_root: &Path,
	target: &SnapshotTarget,
	path: &Path,
) -> MonochangeResult<Option<String>> {
	read_text_file_from_target_with_limit(repo_root, target, path, MAX_SNAPSHOT_FILE_SIZE)
}

fn read_text_file_from_target_with_limit(
	repo_root: &Path,
	target: &SnapshotTarget,
	path: &Path,
	max_file_size: usize,
) -> MonochangeResult<Option<String>> {
	match target {
		SnapshotTarget::WorkingTree => Ok(read_working_tree_text(&repo_root.join(path))),
		SnapshotTarget::GitRevision(revision) => {
			let object = format!("{revision}:{}", path.to_string_lossy());
			if max_file_size == MAX_SNAPSHOT_FILE_SIZE {
				read_text_file_from_git_object(repo_root, &object)
			} else {
				read_text_file_from_git_object_with_limit(repo_root, &object, max_file_size)
			}
		}
		SnapshotTarget::GitIndex => {
			let object = format!(":{}", path.to_string_lossy());
			if max_file_size == MAX_SNAPSHOT_FILE_SIZE {
				read_text_file_from_git_object(repo_root, &object)
			} else {
				read_text_file_from_git_object_with_limit(repo_root, &object, max_file_size)
			}
		}
	}
}

fn read_text_file_from_git_object(
	repo_root: &Path,
	object: &str,
) -> MonochangeResult<Option<String>> {
	read_text_file_from_git_object_with_limit(repo_root, object, MAX_SNAPSHOT_FILE_SIZE)
}

fn read_text_file_from_git_object_with_limit(
	repo_root: &Path,
	object: &str,
	max_file_size: usize,
) -> MonochangeResult<Option<String>> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["show", object])
		.output()
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to run git show {object}: {error}"))
		})?;

	if !output.status.success() {
		return Ok(None);
	}

	if output.stdout.len() > max_file_size {
		return Ok(None);
	}

	Ok(String::from_utf8(output.stdout).ok())
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
