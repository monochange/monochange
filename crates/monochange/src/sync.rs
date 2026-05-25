//! Synchronize internal dependency version references across workspace packages.
//!
//! The `mc sync versions` command updates internal (workspace) dependency
//! references so they match each package's canonical version with the
//! appropriate constraint prefix for the ecosystem.
//!
//! Currently supported ecosystems: Dart, npm.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use monochange_core::DependencySyncChange;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::VersionStrategy;

use crate::discover_workspace;

/// Result of synchronizing internal dependency versions across a workspace.
pub struct SyncResult {
	/// Files that were changed, with the changes made in each.
	pub changes: Vec<FileSyncResult>,
}

/// Changes made to a single file.
pub struct FileSyncResult {
	/// Relative path from workspace root.
	pub path: String,
	/// Individual dependency changes.
	pub changes: Vec<DependencySyncChange>,
}

/// Discover all workspace package versions and update internal dependency
/// references to match canonical versions.
///
/// This is the top-level orchestration function for `mc sync versions`.
/// It discovers packages, builds a version map, then updates each package's
/// manifest to sync internal dep references.
pub fn sync_workspace_versions(
	root: &Path,
	strategy: VersionStrategy,
	dry_run: bool,
) -> MonochangeResult<SyncResult> {
	let discovery = discover_workspace(root)?;
	let mut all_changes: Vec<FileSyncResult> = Vec::new();

	// Build a map of package name -> canonical_version from discovery results.
	// Dependencies reference names, not ids.
	let version_map: BTreeMap<String, String> = discovery
		.packages
		.iter()
		.filter_map(|package| {
			package
				.current_version
				.as_ref()
				.map(|v| (package.name.clone(), v.to_string()))
		})
		.collect();

	if version_map.is_empty() {
		return Ok(SyncResult {
			changes: Vec::new(),
		});
	}

	// Collect all workspace package names for identifying internal deps.
	let workspace_package_names: BTreeSet<String> =
		discovery.packages.iter().map(|p| p.name.clone()).collect();

	// Process each package's manifest.
	for package in &discovery.packages {
		let ecosystem = package.ecosystem;

		// Only sync ecosystems that have sync support.
		let changes = match ecosystem {
			Ecosystem::Dart => {
				let manifest_path = root.join(&package.manifest_path);
				if !manifest_path.exists() {
					continue;
				}
				let contents = std::fs::read_to_string(&manifest_path).map_err(|e| {
					MonochangeError::Io(format!("failed to read {}: {e}", manifest_path.display()))
				})?;
				monochange_dart::sync_internal_dependency_versions(
					&contents,
					&version_map,
					&workspace_package_names,
					strategy,
				)?
			}
			Ecosystem::Npm => {
				let manifest_path = root.join(&package.manifest_path);
				if !manifest_path.exists() {
					continue;
				}
				let contents = std::fs::read_to_string(&manifest_path).map_err(|e| {
					MonochangeError::Io(format!("failed to read {}: {e}", manifest_path.display()))
				})?;
				monochange_npm::sync_internal_dependency_versions(
					&contents,
					&version_map,
					&workspace_package_names,
					strategy,
				)?
			}
			_ => Vec::new(),
		};

		if changes.is_empty() {
			continue;
		}

		if !dry_run {
			// Apply changes and write back.
			let updated_contents =
				apply_sync_changes(root, &package.manifest_path, &changes, ecosystem)?;
			let manifest_path = root.join(&package.manifest_path);
			std::fs::write(&manifest_path, updated_contents).map_err(|e| {
				MonochangeError::Io(format!("failed to write {}: {e}", manifest_path.display()))
			})?;
		}

		all_changes.push(FileSyncResult {
			path: package.manifest_path.to_string_lossy().to_string(),
			changes,
		});
	}

	Ok(SyncResult {
		changes: all_changes,
	})
}

/// Apply detected changes to manifest contents using the ecosystem-specific
/// update function.
fn apply_sync_changes(
	root: &Path,
	manifest_path: &Path,
	changes: &[DependencySyncChange],
	ecosystem: Ecosystem,
) -> MonochangeResult<String> {
	let full_path = root.join(manifest_path);
	let contents = std::fs::read_to_string(&full_path)
		.map_err(|e| MonochangeError::Io(format!("failed to read {}: {e}", full_path.display())))?;

	// Build versioned_deps map from changes for the existing update functions.
	let versioned_deps: BTreeMap<String, String> = changes
		.iter()
		.map(|c| (c.dependency_name.clone(), c.new_value.clone()))
		.collect();

	match ecosystem {
		Ecosystem::Dart => {
			let fields = monochange_dart::default_dependency_fields();
			monochange_dart::update_manifest_text(&contents, None, fields, &versioned_deps)
		}
		// npm uses JSON manifest updates via core.
		Ecosystem::Npm => {
			let fields = monochange_npm::default_dependency_fields();
			monochange_core::update_json_manifest_text(&contents, None, fields, &versioned_deps)
		}
		_ => Ok(contents),
	}
}
