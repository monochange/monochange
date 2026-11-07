//! Cargo workspace graph resolution via `cargo metadata`.
//!
//! `cargo metadata --format-version 1 --no-deps --offline` is the authoritative
//! Cargo source: its `workspace_members` include unpublished crates and crates
//! nested at any depth, and each member's `dependencies[]` reports every
//! dependency kind — normal, dev, build, and `[target.'cfg(..)'.dependencies]`
//! — with `foo.workspace = true` inheritance and renames already resolved into
//! absolute `path` values. The command runs offline with no dependency
//! resolution, so the gate job never downloads the crates.io index. Parsing
//! lives in this submodule so the decision logic in `mod.rs` stays independent
//! of the wire format.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Deserialize;

use super::WorkspaceGraph;

/// Resolve the Cargo workspace graph via `cargo metadata --no-deps --offline`.
///
/// Returns `Ok(None)` when the root has no Cargo manifest, leaving manifest
/// discovery as the source for non-Cargo ecosystems. When a manifest exists
/// but Cargo cannot describe the workspace, cargo's own diagnostics come back
/// as an error: silently falling back to manifest discovery would hide a
/// broken workspace from the gate.
pub(super) fn cargo_metadata_graph(root: &Path) -> MonochangeResult<Option<WorkspaceGraph>> {
	if !root.join("Cargo.toml").is_file() {
		return Ok(None);
	}

	let output = match ProcessCommand::new("cargo")
		.current_dir(root)
		.args([
			"metadata",
			"--format-version",
			"1",
			"--no-deps",
			"--offline",
		])
		.output()
	{
		Ok(output) => output,
		// patch-coverage:ignore-start -- spawning cargo only fails when the binary is missing from PATH.
		Err(error) => return Err(metadata_error(root, &error.to_string())),
		// patch-coverage:ignore-end
	};

	if !output.status.success() {
		return Err(metadata_error(root, &decode(&output.stderr)));
	}

	let metadata: CargoMetadata = match serde_json::from_slice(&output.stdout) {
		Ok(metadata) => metadata,
		// patch-coverage:ignore-start -- a successful `cargo metadata` exit always emits valid JSON.
		Err(error) => {
			return Err(metadata_error(
				root,
				&format!("invalid JSON output: {error}"),
			));
		} // patch-coverage:ignore-end
	};

	Ok(Some(build_graph(&metadata)))
}

/// Turn the metadata payload into packages plus internal dependency edges.
fn build_graph(metadata: &CargoMetadata) -> WorkspaceGraph {
	let members: BTreeSet<&str> = metadata
		.workspace_members
		.iter()
		.map(String::as_str)
		.collect();
	let member_packages: Vec<&MetadataPackage> = metadata
		.packages
		.iter()
		.filter(|package| members.contains(package.id.as_str()))
		.collect();
	// Member manifest directory → member package name, so a dependency entry
	// resolves to its target without trusting the declared dependency name
	// (renames, `workspace = true` inheritance, and dependency kinds are all
	// already flattened by cargo into one `path`-carrying entry).
	let names_by_dir: BTreeMap<&Path, &str> = member_packages
		.iter()
		.map(|package| (manifest_dir(&package.manifest_path), package.name.as_str()))
		.collect();
	let mut graph = WorkspaceGraph::empty();

	for package in &member_packages {
		let rel_dir = relative_manifest_dir(&package.manifest_path, &metadata.workspace_root);

		graph.insert_package(package.name.as_str(), rel_dir);
	}

	for package in &member_packages {
		for dependency in &package.dependencies {
			let Some(path) = &dependency.path else {
				continue;
			};

			if let Some(target) = names_by_dir.get(dependency_dir(path).as_path()) {
				graph
					.deps
					.entry(package.name.clone())
					.or_default()
					.insert(target.to_string());
			}
		}
	}

	graph
}

/// Directory of a metadata manifest path.
fn manifest_dir(manifest_path: &str) -> &Path {
	Path::new(manifest_path)
		.parent()
		.unwrap_or_else(|| Path::new(""))
}

/// Directory a dependency `path` entry points at; cargo emits the member
/// directory itself, and older cargo versions may name the manifest file.
fn dependency_dir(path: &str) -> PathBuf {
	let parsed = Path::new(path);

	if parsed.file_name().is_some_and(|name| name == "Cargo.toml")
		&& let Some(dir) = parsed.parent()
	{
		return dir.to_path_buf();
	}

	parsed.to_path_buf()
}

/// Build a `MonochangeError` carrying cargo's own diagnostics.
fn metadata_error(root: &Path, detail: &str) -> MonochangeError {
	MonochangeError::Io(format!(
		"cargo metadata failed in `{}`: {detail}",
		root.display()
	))
}

/// Decode cargo's stderr for an error message.
fn decode(bytes: &[u8]) -> String {
	String::from_utf8_lossy(bytes).trim().to_string()
}

/// Compute a manifest's directory relative to the workspace root.
fn relative_manifest_dir(manifest_path: &str, workspace_root: &str) -> String {
	Path::new(manifest_path)
		.parent()
		.and_then(|dir| dir.strip_prefix(workspace_root).ok())
		.map(|dir| dir.to_string_lossy().replace('\\', "/"))
		.unwrap_or_default()
}

/// `cargo metadata --format-version 1` payload restricted to graph fields.
#[derive(Debug, Deserialize)]
struct CargoMetadata {
	#[serde(default)]
	workspace_members: Vec<String>,
	#[serde(default)]
	packages: Vec<MetadataPackage>,
	workspace_root: String,
}

/// One `packages` entry from `cargo metadata`.
#[derive(Debug, Deserialize)]
struct MetadataPackage {
	id: String,
	name: String,
	manifest_path: String,
	#[serde(default)]
	dependencies: Vec<MetadataDependency>,
}

/// One `packages[].dependencies[]` entry from `cargo metadata`.
#[derive(Debug, Deserialize)]
struct MetadataDependency {
	#[serde(default)]
	path: Option<String>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn dependency_dir_accepts_directories_and_manifest_files() {
		assert_eq!(
			dependency_dir("/repo/crates/gallery"),
			PathBuf::from("/repo/crates/gallery")
		);
		assert_eq!(
			dependency_dir("/repo/crates/gallery/Cargo.toml"),
			PathBuf::from("/repo/crates/gallery")
		);
	}
}
