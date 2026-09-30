//! Synchronize internal dependency version references across workspace packages.
//!
//! The `monochange versions` command updates internal (workspace) dependency
//! references so they match each package's canonical version with the
//! appropriate constraint prefix for the ecosystem.
//!
//! Supported ecosystems: Cargo, Dart, Deno, Go, npm, and Python.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::DependencySyncChange;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::VersionStrategy;
use semver::Version;
use serde::Serialize;
use serde_json::Value as JsonValue;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::Value as TomlValue;

use crate::OutputFormat;
use crate::discover_workspace;
use crate::output::text::Outcome;
use crate::output::text::TableCell;
use crate::output::text::TextReport;
use crate::output::text::TextTheme;
use crate::output::text::Tone;
use crate::output::text::plural;
use crate::workspace_ops::discover_workspace_with_configuration;
use crate::workspace_ops::load_sorted_tags_sync;
use crate::workspace_ops::seed_versions_from_tag_list;

/// Flat package and group version inventory for a workspace.
pub type VersionInventory = BTreeMap<String, String>;

/// Result of synchronizing internal dependency versions across a workspace.
#[derive(Debug, Serialize)]
pub struct SyncResult {
	/// Whether files were written to disk.
	pub applied: bool,
	/// Strategy requested by the CLI.
	pub strategy: VersionStrategy,
	/// Files that were changed, with the changes made in each.
	pub changes: Vec<FileSyncResult>,
	/// Workspace package manifests skipped because their ecosystem is not supported yet.
	pub skipped: Vec<SkippedSyncPackage>,
}

/// Planned version synchronization work.
#[derive(Debug, Serialize)]
pub struct VersionSyncPlan {
	/// Strategy requested by the CLI.
	pub strategy: VersionStrategy,
	/// Files with pending dependency constraint edits.
	pub files: Vec<VersionSyncPlanFile>,
	/// Workspace package manifests skipped because their ecosystem is not supported yet.
	pub skipped: Vec<SkippedSyncPackage>,
}

/// Planned changes for one manifest file.
#[derive(Debug, Serialize)]
pub struct VersionSyncPlanFile {
	/// Relative path from workspace root.
	pub path: String,
	/// Manifest ecosystem.
	pub ecosystem: Ecosystem,
	/// Individual dependency changes.
	pub changes: Vec<DependencySyncChange>,
	#[serde(skip)]
	pub(crate) manifest_path: PathBuf,
	#[serde(skip)]
	pub(crate) contents: String,
}

/// Changes made to a single file.
#[derive(Debug, Serialize)]
pub struct FileSyncResult {
	/// Relative path from workspace root.
	pub path: String,
	/// Manifest ecosystem.
	pub ecosystem: Ecosystem,
	/// Individual dependency changes.
	pub changes: Vec<DependencySyncChange>,
}

/// A workspace package skipped by `monochange versions`.
#[derive(Clone, Debug, Serialize)]
pub struct SkippedSyncPackage {
	/// Relative manifest path.
	pub path: String,
	/// Package name from the manifest.
	pub package_name: String,
	/// Package ecosystem.
	pub ecosystem: Ecosystem,
	/// Reason the package was skipped.
	pub reason: String,
}

type DetectVersionSyncChanges = fn(
	&str,
	&BTreeMap<String, String>,
	&BTreeSet<String>,
	&BTreeMap<String, String>,
	VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>>;

type ApplyVersionSyncChanges = fn(&str, &[DependencySyncChange]) -> MonochangeResult<String>;

#[derive(Clone, Copy)]
struct VersionSyncAdapter {
	ecosystem: Ecosystem,
	detect_changes: DetectVersionSyncChanges,
	apply_changes: ApplyVersionSyncChanges,
}

impl VersionSyncAdapter {
	fn for_ecosystem(ecosystem: Ecosystem) -> Self {
		match ecosystem {
			Ecosystem::Cargo => {
				Self {
					ecosystem,
					detect_changes: detect_cargo_changes,
					apply_changes: apply_cargo_changes,
				}
			}
			Ecosystem::Dart => {
				Self {
					ecosystem,
					detect_changes: detect_dart_changes,
					apply_changes: apply_dart_changes,
				}
			}
			Ecosystem::Deno => {
				Self {
					ecosystem,
					detect_changes: detect_deno_changes,
					apply_changes: apply_deno_changes,
				}
			}
			Ecosystem::Go => {
				Self {
					ecosystem,
					detect_changes: detect_go_changes,
					apply_changes: apply_go_changes,
				}
			}
			Ecosystem::Npm => {
				Self {
					ecosystem,
					detect_changes: detect_npm_changes,
					apply_changes: apply_npm_changes,
				}
			}
			Ecosystem::Python => {
				Self {
					ecosystem,
					detect_changes: detect_python_changes,
					apply_changes: apply_python_changes,
				}
			}
			#[allow(unreachable_patterns)]
			_ => panic!("unsupported ecosystem: {ecosystem:?}"),
		}
	}
}

/// Discover all workspace package versions and update internal dependency
/// references to match canonical versions.
///
/// This is the top-level orchestration function for `monochange versions`.
/// It builds a plan first, then applies that plan unless `dry_run` is set.
pub fn sync_workspace_versions(
	root: &Path,
	strategy: VersionStrategy,
	dry_run: bool,
) -> MonochangeResult<SyncResult> {
	let plan = plan_workspace_versions(root, strategy)?;
	apply_version_sync_plan(&plan, !dry_run)
}

/// Build a version sync plan without writing files.
///
/// Tag-versioned packages (currently Go) carry no version in their manifests,
/// so the plan seeds their canonical versions from release tags — the same
/// baselines release planning resolves — before detecting stale internal
/// dependency references. Without this pass a pure Go workspace would have an
/// empty version map and `versions sync` would never report anything.
pub fn plan_workspace_versions(
	root: &Path,
	strategy: VersionStrategy,
) -> MonochangeResult<VersionSyncPlan> {
	let configuration = monochange_config::load_workspace_configuration(root)?;
	let mut discovery = discover_workspace_with_configuration(root, &configuration)?;
	seed_versions_from_tag_list(&configuration, &mut discovery, &load_sorted_tags_sync(root));
	plan_discovered_workspace_versions(root, strategy, &discovery)
}

pub(crate) fn plan_discovered_workspace_versions(
	root: &Path,
	strategy: VersionStrategy,
	discovery: &DiscoveryReport,
) -> MonochangeResult<VersionSyncPlan> {
	plan_discovered_workspace_versions_with_overrides(
		root,
		strategy,
		discovery,
		&package_version_map(discovery),
		&BTreeMap::new(),
	)
}

pub(crate) fn plan_discovered_workspace_versions_with_overrides(
	root: &Path,
	strategy: VersionStrategy,
	discovery: &DiscoveryReport,
	version_map: &BTreeMap<String, String>,
	manifest_contents: &BTreeMap<PathBuf, String>,
) -> MonochangeResult<VersionSyncPlan> {
	let workspace_package_names = workspace_package_names(discovery);
	let workspace_go_module_paths = workspace_go_module_paths(discovery);
	let mut files = Vec::with_capacity(discovery.packages.len());
	let skipped = Vec::new();

	for package in &discovery.packages {
		let adapter = VersionSyncAdapter::for_ecosystem(package.ecosystem);

		if version_map.is_empty() {
			continue;
		}

		let manifest_path = root.join(&package.manifest_path);
		let contents = manifest_contents
			.get(&manifest_path)
			.cloned()
			.map_or_else(|| read_manifest(&manifest_path), Ok)?;
		let changes = (adapter.detect_changes)(
			&contents,
			version_map,
			&workspace_package_names,
			&workspace_go_module_paths,
			strategy,
		)
		.map_err(|error| {
			sync_context_error("detect", adapter.ecosystem, &package.manifest_path, &error)
		})?;

		if changes.is_empty() {
			continue;
		}

		files.push(VersionSyncPlanFile {
			path: package.manifest_path.to_string_lossy().to_string(),
			ecosystem: package.ecosystem,
			changes,
			manifest_path,
			contents,
		});
	}

	Ok(VersionSyncPlan {
		strategy,
		files,
		skipped,
	})
}

/// Apply a version sync plan. Pass `write_files = false` for dry-run behavior.
pub fn apply_version_sync_plan(
	plan: &VersionSyncPlan,
	write_files: bool,
) -> MonochangeResult<SyncResult> {
	let mut changes = Vec::with_capacity(plan.files.len());

	for file in &plan.files {
		if write_files {
			let updated_contents =
				apply_sync_changes(&file.contents, &file.changes, file.ecosystem).map_err(
					|error| {
						sync_context_error("apply", file.ecosystem, Path::new(&file.path), &error)
					},
				)?;
			write_manifest(&file.manifest_path, updated_contents)?;
		}

		changes.push(FileSyncResult {
			path: file.path.clone(),
			ecosystem: file.ecosystem,
			changes: file.changes.clone(),
		});
	}

	Ok(SyncResult {
		applied: write_files,
		strategy: plan.strategy,
		changes,
		skipped: plan.skipped.clone(),
	})
}

fn package_version_map(discovery: &DiscoveryReport) -> BTreeMap<String, String> {
	discovery
		.packages
		.iter()
		.filter_map(|package| {
			package
				.current_version
				.as_ref()
				.map(|version| (package.name.clone(), version.to_string()))
		})
		.collect()
}

fn workspace_package_names(discovery: &DiscoveryReport) -> BTreeSet<String> {
	discovery
		.packages
		.iter()
		.map(|package| package.name.clone())
		.collect()
}

/// Map a workspace package name to the Go module path declared by its `go.mod`.
///
/// A Go module's identity is its full module path (`github.com/acme/core`), not
/// the derived short name (`core`). Matching a `require` directive by short name
/// alone would rewrite an unrelated module that happens to end in the same
/// segment, so `detect_go_changes` resolves against these paths.
fn workspace_go_module_paths(discovery: &DiscoveryReport) -> BTreeMap<String, String> {
	discovery
		.packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Go)
		.filter_map(|package| {
			let manifest = std::fs::read_to_string(&package.manifest_path).ok()?;
			let module_path = manifest.lines().find_map(|line| {
				line.trim()
					.strip_prefix("module ")
					.map(|rest| rest.trim().trim_matches('"').to_string())
			})?;
			Some((package.name.clone(), module_path))
		})
		.collect()
}

pub(crate) fn sync_context_error(
	operation: &str,
	ecosystem: Ecosystem,
	path: &Path,
	error: &MonochangeError,
) -> MonochangeError {
	MonochangeError::Config(format!(
		"failed to {operation} {:?} version sync changes for {}: {error}",
		ecosystem,
		path.display()
	))
}

pub(crate) fn version_prefix_for_strategy(
	ecosystem: Ecosystem,
	strategy: VersionStrategy,
) -> &'static str {
	if strategy == VersionStrategy::Exact {
		return "";
	}
	if matches!(ecosystem, Ecosystem::Cargo | Ecosystem::Go) {
		return if strategy == VersionStrategy::Compatible {
			">="
		} else {
			""
		};
	}
	match ecosystem {
		Ecosystem::Dart => monochange_dart::default_dependency_version_prefix(),
		Ecosystem::Deno => monochange_deno::default_dependency_version_prefix(),
		Ecosystem::Npm => monochange_npm::default_dependency_version_prefix(),
		Ecosystem::Python => monochange_python::default_dependency_version_prefix(),
		// Cargo and Go don't use version prefixes for caret/compatible strategies.
		Ecosystem::Cargo | Ecosystem::Go => "",
		#[allow(unreachable_patterns)]
		_ => panic!("unsupported ecosystem: {ecosystem:?}"),
	}
}

pub(crate) fn target_constraint(
	ecosystem: Ecosystem,
	version: &str,
	strategy: VersionStrategy,
) -> String {
	let version = if ecosystem == Ecosystem::Go && !version.starts_with('v') {
		format!("v{version}")
	} else {
		version.to_string()
	};
	format!(
		"{}{}",
		version_prefix_for_strategy(ecosystem, strategy),
		version
	)
}

fn push_change(
	changes: &mut Vec<DependencySyncChange>,
	dependency_name: &str,
	section: &str,
	old_value: &str,
	new_value: String,
) {
	if old_value != new_value {
		changes.push(DependencySyncChange {
			dependency_name: dependency_name.to_string(),
			section: section.to_string(),
			old_value: old_value.to_string(),
			new_value,
		});
	}
}

fn parse_python_dependency_name(spec: &str) -> Option<String> {
	let name = spec
		.chars()
		.take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
		.collect::<String>();
	(!name.is_empty()).then_some(name)
}

pub(crate) fn normalize_python_package_name(name: &str) -> String {
	let mut result = String::with_capacity(name.len());
	let mut prev_was_separator = false;
	for ch in name.chars() {
		if matches!(ch, '-' | '_' | '.') {
			if !prev_was_separator {
				result.push('-');
			}
			prev_was_separator = true;
		} else {
			result.push(ch.to_ascii_lowercase());
			prev_was_separator = false;
		}
	}
	result
}

pub(crate) fn extract_python_version_constraint(spec: &str, name: &str) -> Option<String> {
	let mut rest = spec.get(name.len()..)?.trim_start();
	if rest.starts_with('[') {
		let extras_end = rest.find(']')? + 1;
		rest = rest.get(extras_end..)?.trim_start();
	}
	let constraint = rest.split(';').next().unwrap_or_default().trim();
	(!constraint.is_empty()).then(|| constraint.to_string())
}

fn detect_toml_dependency_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	strategy: VersionStrategy,
	fields: &[&str],
	ecosystem: Ecosystem,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let document = contents.parse::<DocumentMut>().map_err(|error| {
		MonochangeError::Config(format!("failed to parse toml for sync: {error}"))
	})?;
	let mut changes = Vec::new();

	for field in fields {
		let Some(table) = document.get(field).and_then(Item::as_table_like) else {
			continue;
		};
		for dep_name in workspace_package_names {
			let Some(item) = table.get(dep_name) else {
				continue;
			};
			let Some(version) = version_map.get(dep_name) else {
				continue;
			};
			let new_value = target_constraint(ecosystem, version, strategy);
			if let Some(old_value) = item.as_str() {
				push_change(&mut changes, dep_name, field, old_value, new_value);
				continue;
			}
			let Some(old_value) = item
				.as_inline_table()
				.and_then(|table| table.get("version"))
				.and_then(TomlValue::as_str)
			else {
				continue;
			};
			push_change(&mut changes, dep_name, field, old_value, new_value);
		}
	}

	Ok(changes)
}

pub(crate) fn detect_cargo_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	_workspace_go_module_paths: &BTreeMap<String, String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	detect_toml_dependency_changes(
		contents,
		version_map,
		workspace_package_names,
		strategy,
		monochange_cargo::default_dependency_fields(),
		Ecosystem::Cargo,
	)
}

pub(crate) fn detect_python_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	_workspace_go_module_paths: &BTreeMap<String, String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let document = contents.parse::<DocumentMut>().map_err(|error| {
		MonochangeError::Config(format!("failed to parse pyproject.toml for sync: {error}"))
	})?;
	let mut changes = Vec::new();
	let Some(project) = document.get("project").and_then(Item::as_table_like) else {
		return Ok(changes);
	};
	let Some(deps) = project.get("dependencies").and_then(Item::as_array) else {
		return Ok(changes);
	};

	for dep in deps.iter().filter_map(TomlValue::as_str) {
		let Some(dep_name) = parse_python_dependency_name(dep) else {
			continue;
		};
		let normalized_name = normalize_python_package_name(&dep_name);
		let Some(workspace_name) = workspace_package_names
			.iter()
			.find(|name| normalize_python_package_name(name) == normalized_name)
		else {
			continue;
		};
		let Some(version) = version_map.get(workspace_name) else {
			continue;
		};
		let new_value = target_constraint(Ecosystem::Python, version, strategy);
		let old_value = extract_python_version_constraint(dep, &dep_name).unwrap_or_default();
		push_change(
			&mut changes,
			workspace_name,
			"dependencies",
			&old_value,
			new_value,
		);
	}

	Ok(changes)
}

pub(crate) fn detect_go_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	workspace_go_module_paths: &BTreeMap<String, String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let contents = std::str::from_utf8(contents.as_bytes())
		.map_err(|error| MonochangeError::Config(format!("failed to read go.mod text: {error}")))?;
	let mut changes = Vec::new();
	for line in contents.lines() {
		let trimmed = line.trim();
		let parts = trimmed.split_whitespace().collect::<Vec<_>>();
		let (Some(module_path), Some(old_value)) = (match parts.as_slice() {
			["require", name, version] | [name, version] | [name, version, "//", "indirect"] => {
				(Some(*name), Some(*version))
			}
			_ => (None, None),
		}) else {
			continue;
		};
		let Some(workspace_name) = resolve_go_workspace_package(
			module_path,
			workspace_package_names,
			workspace_go_module_paths,
		) else {
			continue;
		};
		let Some(version) = version_map.get(&workspace_name) else {
			continue;
		};
		push_change(
			&mut changes,
			module_path,
			"require",
			old_value,
			target_constraint(Ecosystem::Go, version, strategy),
		);
	}
	Ok(changes)
}

/// Resolve a `require` module path to a workspace package name.
///
/// A Go module is identified by its full module path (`github.com/acme/core`),
/// which the workspace's own `go.mod` declares. Matching by derived short name
/// instead would treat any module ending in the same segment as a workspace
/// member — `github.com/other/core` would be mistaken for `core` and rewritten
/// to the workspace's version, silently corrupting an unrelated pin. So only an
/// exact module-path match resolves; a package named literally `core` still
/// matches a `require core` line through the short-name set.
pub(crate) fn resolve_go_workspace_package(
	module_path: &str,
	workspace_package_names: &BTreeSet<String>,
	workspace_go_module_paths: &BTreeMap<String, String>,
) -> Option<String> {
	if let Some((name, _)) = workspace_go_module_paths
		.iter()
		.find(|(_, path)| path.as_str() == module_path)
	{
		return Some(name.clone());
	}
	// A module whose own path is a bare name (`module core`) is identified by
	// that name directly.
	workspace_package_names
		.contains(module_path)
		.then(|| module_path.to_string())
}

pub(crate) fn detect_deno_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	_workspace_go_module_paths: &BTreeMap<String, String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let json = monochange_deno::parse_manifest_contents(contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse deno json for sync: {error}"))
	})?;
	let mut changes = Vec::new();
	for section in ["imports", "dependencies"] {
		let Some(deps) = json.get(section).and_then(JsonValue::as_object) else {
			continue;
		};
		for dep_name in workspace_package_names {
			let Some(old_value) = deps.get(dep_name).and_then(JsonValue::as_str) else {
				continue;
			};
			let Some(version) = version_map.get(dep_name) else {
				continue;
			};
			push_change(
				&mut changes,
				dep_name,
				section,
				old_value,
				target_constraint(Ecosystem::Deno, version, strategy),
			);
		}
	}
	Ok(changes)
}

pub(crate) fn detect_dart_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	_workspace_go_module_paths: &BTreeMap<String, String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	monochange_dart::sync_internal_dependency_versions(
		contents,
		version_map,
		workspace_package_names,
		strategy,
	)
}

fn detect_npm_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	_workspace_go_module_paths: &BTreeMap<String, String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	monochange_npm::sync_internal_dependency_versions(
		contents,
		version_map,
		workspace_package_names,
		strategy,
	)
}

fn apply_cargo_changes(
	contents: &str,
	changes: &[DependencySyncChange],
) -> MonochangeResult<String> {
	let fields = monochange_cargo::default_dependency_fields();
	let versioned_deps = versioned_deps_from_changes(changes);
	monochange_cargo::update_versioned_file_text(
		contents,
		monochange_cargo::CargoVersionedFileKind::Manifest,
		fields,
		None,
		None,
		&versioned_deps,
		&BTreeMap::new(),
	)
	.map_err(|error| MonochangeError::Config(format!("failed to update Cargo.toml: {error}")))
}

fn apply_deno_changes(
	contents: &str,
	changes: &[DependencySyncChange],
) -> MonochangeResult<String> {
	let fields = &["imports", "dependencies"];
	let versioned_deps = versioned_deps_from_changes(changes);
	monochange_core::update_json_manifest_text(contents, None, fields, &versioned_deps)
}

fn apply_go_changes(contents: &str, changes: &[DependencySyncChange]) -> MonochangeResult<String> {
	let contents = std::str::from_utf8(contents.as_bytes())
		.map_err(|error| MonochangeError::Config(format!("failed to read go.mod text: {error}")))?;
	// Changes are keyed by the full module path from the `require` directive, so
	// `update_go_mod_text` matches each line by exact module path and never
	// rewrites an external module that happens to share a last segment with a
	// workspace package.
	let versioned_deps = versioned_deps_from_changes(changes);
	Ok(monochange_go::update_go_mod_text(contents, &versioned_deps))
}

fn apply_python_changes(
	contents: &str,
	changes: &[DependencySyncChange],
) -> MonochangeResult<String> {
	monochange_python::update_versioned_file_text(
		contents,
		monochange_python::PythonVersionedFileKind::Manifest,
		None,
		&versioned_deps_from_changes(changes),
	)
	.map_err(|error| MonochangeError::Config(format!("failed to update pyproject.toml: {error}")))
}

fn apply_dart_changes(
	contents: &str,
	changes: &[DependencySyncChange],
) -> MonochangeResult<String> {
	let fields = monochange_dart::default_dependency_fields();
	let versioned_deps = versioned_deps_from_changes(changes);
	monochange_dart::update_manifest_text(contents, None, fields, &versioned_deps)
}

fn apply_npm_changes(contents: &str, changes: &[DependencySyncChange]) -> MonochangeResult<String> {
	let fields = monochange_npm::default_dependency_fields();
	let versioned_deps = versioned_deps_from_changes(changes);
	monochange_core::update_json_manifest_text(contents, None, fields, &versioned_deps)
}

fn versioned_deps_from_changes(changes: &[DependencySyncChange]) -> BTreeMap<String, String> {
	changes
		.iter()
		.map(|change| (change.dependency_name.clone(), change.new_value.clone()))
		.collect()
}

pub(crate) fn read_manifest(path: &Path) -> MonochangeResult<String> {
	std::fs::read_to_string(path)
		.map_err(|error| MonochangeError::Io(format!("failed to read {}: {error}", path.display())))
}

pub(crate) fn write_manifest(path: &Path, contents: String) -> MonochangeResult<()> {
	std::fs::write(path, contents).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})
}

/// Apply detected changes to manifest contents using the ecosystem-specific
/// update function.
///
/// Accepts the manifest contents as a string so the function can be tested
/// without file I/O.
pub fn apply_sync_changes(
	contents: &str,
	changes: &[DependencySyncChange],
	ecosystem: Ecosystem,
) -> MonochangeResult<String> {
	let adapter = VersionSyncAdapter::for_ecosystem(ecosystem);
	(adapter.apply_changes)(contents, changes)
}

/// Format the sync result as a human-readable output string.
///
/// This function is separated from the main dispatch so it can be tested
/// independently.
///
/// `check_only` marks the read-only `monochange versions` check, which points
/// at `monochange versions sync` when constraints need updating.
pub(crate) fn format_sync_result(
	root: &Path,
	result: &SyncResult,
	dry_run: bool,
	check_only: bool,
) -> String {
	let mut text = TextReport::new(TextTheme::for_stdout());
	let change_count = result
		.changes
		.iter()
		.map(|file_result| file_result.changes.len())
		.sum::<usize>();
	let strategy = format!("strategy {}", strategy_name(result.strategy));
	if change_count == 0 {
		text.headline(
			Outcome::Success,
			"Internal dependency constraints are already in sync",
			&[strategy],
		);
	} else {
		let mut details = vec![
			format!("in {}", plural(result.changes.len(), "file", "files")),
			strategy,
		];
		if check_only {
			details.push("no files were modified".to_string());
		} else if dry_run {
			details.push("dry-run, no files were modified".to_string());
		}
		let (outcome, verb) = if dry_run {
			(Outcome::Neutral, "Would update")
		} else {
			(Outcome::Success, "Updated")
		};
		text.headline(
			outcome,
			&format!(
				"{verb} {}",
				plural(
					change_count,
					"dependency constraint",
					"dependency constraints"
				)
			),
			&details,
		);
	}

	let root_prefix = format!("{}/", root.display());
	for file_result in &result.changes {
		text.paragraph(
			file_result.path.trim_start_matches(&root_prefix),
			Tone::Heading,
		);
		let rows = file_result
			.changes
			.iter()
			.map(|change| {
				vec![
					TableCell::new(&change.dependency_name, Tone::Plain),
					TableCell::new(
						format!("{} → {}", change.old_value, change.new_value),
						Tone::Value,
					),
				]
			})
			.collect::<Vec<_>>();
		text.table(&rows);
	}

	if !result.skipped.is_empty() {
		text.section("Skipped unsupported ecosystems", Some(result.skipped.len()));
		text.list(
			result.skipped.iter().map(|skipped| {
				format!(
					"{} ({:?}): {}",
					skipped.path.trim_start_matches(&root_prefix),
					skipped.ecosystem,
					skipped.reason
				)
			}),
			usize::MAX,
		);
	}

	if check_only && change_count > 0 {
		text.paragraph(
			"Run `monochange versions sync` to write these changes.",
			Tone::Plain,
		);
	}
	text.paragraph(
		"Strategy order: package config → ecosystem config → ecosystem default; `--strategy` overrides.",
		Tone::Muted,
	);
	text.render()
}

pub(crate) fn format_sync_result_json(result: &SyncResult, format: OutputFormat) -> String {
	let mut output = format
		.render_json_value(result, "versions sync result")
		.unwrap_or_else(|error| panic!("serialize versions result: {error}"));
	output.push('\n');
	output
}

pub(crate) fn format_sync_result_for_cli(
	root: &Path,
	result: &SyncResult,
	dry_run: bool,
	check_only: bool,
	format: OutputFormat,
) -> String {
	match format {
		OutputFormat::Text | OutputFormat::Markdown => {
			format_sync_result(root, result, dry_run, check_only)
		}
		OutputFormat::Json | OutputFormat::JsonMin => format_sync_result_json(result, format),
	}
}

/// Parse a strategy string from CLI into a `VersionStrategy` enum.
pub fn list_workspace_versions(root: &Path) -> MonochangeResult<VersionInventory> {
	let discovery = discover_workspace(root)?;
	Ok(version_inventory(&discovery))
}

pub(crate) fn version_inventory(discovery: &DiscoveryReport) -> VersionInventory {
	let package_versions_by_id = discovery
		.packages
		.iter()
		.filter_map(|package| {
			package
				.current_version
				.as_ref()
				.map(|version| (package.id.clone(), version.clone()))
		})
		.collect::<BTreeMap<_, _>>();

	let mut inventory = discovery
		.packages
		.iter()
		.filter_map(|package| {
			package.current_version.as_ref().map(|version| {
				let key = package
					.metadata
					.get("config_id")
					.cloned()
					.unwrap_or_else(|| package.id.clone());
				(key, version.to_string())
			})
		})
		.collect::<VersionInventory>();

	for group in &discovery.version_groups {
		if let Some(version) = group_current_version(group.members.iter(), &package_versions_by_id)
		{
			inventory.insert(group.group_id.clone(), version.to_string());
		}
	}

	inventory
}

fn group_current_version<'a>(
	members: impl IntoIterator<Item = &'a String>,
	package_versions_by_id: &BTreeMap<String, Version>,
) -> Option<Version> {
	members
		.into_iter()
		.filter_map(|member| package_versions_by_id.get(member))
		.max()
		.cloned()
}

pub(crate) fn format_version_inventory_for_cli(
	inventory: &VersionInventory,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			format.render_json_value(inventory, "version inventory")
		}
		OutputFormat::Text | OutputFormat::Markdown => {
			Ok(inventory
				.iter()
				.map(|(id, version)| format!("{id}: {version}"))
				.collect::<Vec<_>>()
				.join("\n"))
		}
	}
}

pub(crate) fn parse_strategy(strategy_str: &str) -> VersionStrategy {
	match strategy_str {
		"exact" => VersionStrategy::Exact,
		"caret" => VersionStrategy::Caret,
		"compatible" => VersionStrategy::Compatible,
		_ => VersionStrategy::Default,
	}
}

pub(crate) fn parse_versions_output_format(format_str: &str) -> MonochangeResult<OutputFormat> {
	match format_str {
		"json" => Ok(OutputFormat::Json),
		"json-min" => Ok(OutputFormat::JsonMin),
		"text" => Ok(OutputFormat::Text),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported output format `{other}`"
			)))
		}
	}
}

pub(crate) fn strategy_name(strategy: VersionStrategy) -> &'static str {
	match strategy {
		VersionStrategy::Default => "default",
		VersionStrategy::Exact => "exact",
		VersionStrategy::Caret => "caret",
		VersionStrategy::Compatible => "compatible",
	}
}
