//! Synchronize internal dependency version references across workspace packages.
//!
//! The `mc versions` command updates internal (workspace) dependency
//! references so they match each package's canonical version with the
//! appropriate constraint prefix for the ecosystem.
//!
//! Supported ecosystems: Cargo, Dart, Deno, Go, npm, and Python.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::DependencySyncChange;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::VersionStrategy;
use serde::Serialize;
use serde_json::Value as JsonValue;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::Value as TomlValue;

use crate::discover_workspace;

/// Output format for the `mc versions` command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VersionsOutputFormat {
	/// Human-readable command output.
	Text,
	/// Machine-readable JSON output.
	Json,
}

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

/// A workspace package skipped by `mc versions`.
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
	fn for_ecosystem(ecosystem: Ecosystem) -> Option<Self> {
		match ecosystem {
			Ecosystem::Cargo => {
				Some(Self {
					ecosystem,
					detect_changes: detect_cargo_changes,
					apply_changes: apply_cargo_changes,
				})
			}
			Ecosystem::Dart => {
				Some(Self {
					ecosystem,
					detect_changes: detect_dart_changes,
					apply_changes: apply_dart_changes,
				})
			}
			Ecosystem::Deno => {
				Some(Self {
					ecosystem,
					detect_changes: detect_deno_changes,
					apply_changes: apply_deno_changes,
				})
			}
			Ecosystem::Go => {
				Some(Self {
					ecosystem,
					detect_changes: detect_go_changes,
					apply_changes: apply_go_changes,
				})
			}
			Ecosystem::Npm => {
				Some(Self {
					ecosystem,
					detect_changes: detect_npm_changes,
					apply_changes: apply_npm_changes,
				})
			}
			Ecosystem::Python => {
				Some(Self {
					ecosystem,
					detect_changes: detect_python_changes,
					apply_changes: apply_python_changes,
				})
			}
			_ => None,
		}
	}
}

/// Discover all workspace package versions and update internal dependency
/// references to match canonical versions.
///
/// This is the top-level orchestration function for `mc versions`.
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
pub fn plan_workspace_versions(
	root: &Path,
	strategy: VersionStrategy,
) -> MonochangeResult<VersionSyncPlan> {
	let discovery = discover_workspace(root)?;
	plan_discovered_workspace_versions(root, strategy, &discovery)
}

pub(crate) fn plan_discovered_workspace_versions(
	root: &Path,
	strategy: VersionStrategy,
	discovery: &DiscoveryReport,
) -> MonochangeResult<VersionSyncPlan> {
	let version_map = package_version_map(discovery);
	let workspace_package_names = workspace_package_names(discovery);
	let mut files = Vec::with_capacity(discovery.packages.len());
	let mut skipped = Vec::new();

	for package in &discovery.packages {
		let Some(adapter) = VersionSyncAdapter::for_ecosystem(package.ecosystem) else {
			skipped.push(skipped_package(
				package,
				"ecosystem sync is not implemented yet",
			));
			continue;
		};

		if version_map.is_empty() {
			continue;
		}

		let manifest_path = root.join(&package.manifest_path);
		let contents = read_manifest(&manifest_path)?;
		let changes =
			(adapter.detect_changes)(&contents, &version_map, &workspace_package_names, strategy)
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

fn skipped_package(package: &PackageRecord, reason: &str) -> SkippedSyncPackage {
	SkippedSyncPackage {
		path: package.manifest_path.to_string_lossy().to_string(),
		package_name: package.name.clone(),
		ecosystem: package.ecosystem,
		reason: reason.to_string(),
	}
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

fn version_prefix_for_strategy(ecosystem: Ecosystem, strategy: VersionStrategy) -> &'static str {
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
		_ => "",
	}
}

fn target_constraint(ecosystem: Ecosystem, version: &str, strategy: VersionStrategy) -> String {
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

fn normalize_python_package_name(name: &str) -> String {
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

fn extract_python_version_constraint(spec: &str, name: &str) -> Option<String> {
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
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let contents = std::str::from_utf8(contents.as_bytes())
		.map_err(|error| MonochangeError::Config(format!("failed to read go.mod text: {error}")))?;
	let mut changes = Vec::new();
	for line in contents.lines() {
		let trimmed = line.trim();
		let parts = trimmed.split_whitespace().collect::<Vec<_>>();
		let (Some(dep_name), Some(old_value)) = (match parts.as_slice() {
			["require", name, version] | [name, version] | [name, version, "//", "indirect"] => {
				(Some(*name), Some(*version))
			}
			_ => (None, None),
		}) else {
			continue;
		};
		if !workspace_package_names.contains(dep_name) {
			continue;
		}
		let Some(version) = version_map.get(dep_name) else {
			continue;
		};
		push_change(
			&mut changes,
			dep_name,
			"require",
			old_value,
			target_constraint(Ecosystem::Go, version, strategy),
		);
	}
	Ok(changes)
}

pub(crate) fn detect_deno_changes(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let normalized = monochange_core::strip_json_comments(contents);
	let json: JsonValue = serde_json::from_str(&normalized).map_err(|error| {
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
	let normalized = monochange_core::strip_json_comments(contents);
	monochange_core::update_json_manifest_text(&normalized, None, fields, &versioned_deps)
}

fn apply_go_changes(contents: &str, changes: &[DependencySyncChange]) -> MonochangeResult<String> {
	let contents = std::str::from_utf8(contents.as_bytes())
		.map_err(|error| MonochangeError::Config(format!("failed to read go.mod text: {error}")))?;
	let mut versioned_deps = versioned_deps_from_changes(changes);
	for change in changes {
		if let Some(module_name) = change.dependency_name.rsplit('/').next() {
			versioned_deps.insert(module_name.to_string(), change.new_value.clone());
		}
	}
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
	let Some(adapter) = VersionSyncAdapter::for_ecosystem(ecosystem) else {
		return Ok(contents.to_string());
	};
	(adapter.apply_changes)(contents, changes)
}

/// Format the sync result as a human-readable output string.
///
/// This function is separated from the main dispatch so it can be tested
/// independently.
pub(crate) fn format_sync_result(result: &SyncResult, dry_run: bool, _quiet: bool) -> String {
	if result.changes.is_empty() && result.skipped.is_empty() {
		return String::new();
	}

	let mut output = String::new();
	let action = if dry_run { "would update" } else { "updated" };
	for file_result in &result.changes {
		for change in &file_result.changes {
			let _ = writeln!(
				output,
				"{action} {} → {} in {} ({})",
				change.old_value, change.new_value, change.dependency_name, file_result.path
			);
		}
	}

	if !result.skipped.is_empty() {
		if !output.is_empty() {
			output.push('\n');
		}
		let _ = writeln!(output, "Skipped unsupported ecosystems:");
		for skipped in &result.skipped {
			let _ = writeln!(
				output,
				"- {} ({:?}): {}",
				skipped.path, skipped.ecosystem, skipped.reason
			);
		}
	}

	let _ = writeln!(
		output,
		"\nStrategy: {} (package config → ecosystem config → ecosystem default; --strategy overrides)",
		strategy_name(result.strategy)
	);

	if dry_run {
		output.push_str("\n(dry run — no files were modified)\n");
	}

	output
}

pub(crate) fn format_sync_result_json(result: &SyncResult) -> String {
	let mut output = serde_json::to_string_pretty(result)
		.unwrap_or_else(|error| panic!("serialize versions result: {error}"));
	output.push('\n');
	output
}

pub(crate) fn format_sync_result_for_cli(
	result: &SyncResult,
	dry_run: bool,
	quiet: bool,
	format: VersionsOutputFormat,
) -> String {
	match format {
		VersionsOutputFormat::Text => format_sync_result(result, dry_run, quiet),
		VersionsOutputFormat::Json => format_sync_result_json(result),
	}
}

/// Parse a strategy string from CLI into a `VersionStrategy` enum.
pub(crate) fn parse_strategy(strategy_str: &str) -> VersionStrategy {
	match strategy_str {
		"exact" => VersionStrategy::Exact,
		"caret" => VersionStrategy::Caret,
		"compatible" => VersionStrategy::Compatible,
		_ => VersionStrategy::Default,
	}
}

pub(crate) fn parse_versions_output_format(format_str: &str) -> VersionsOutputFormat {
	match format_str {
		"json" => VersionsOutputFormat::Json,
		_ => VersionsOutputFormat::Text,
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
