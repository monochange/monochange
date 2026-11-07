//! Cargo workspace graph resolution via `cargo metadata`.
//!
//! `cargo metadata --format-version 1` is the authoritative Cargo resolver:
//! its `workspace_members` include unpublished crates and crates nested at
//! any depth, and its resolved graph follows `foo.workspace = true`
//! inheritance, renamed dependencies (`package = "real_name"`), and
//! `[target.'cfg(..)'.dependencies]` sections without any re-implementation
//! here. Parsing lives in this submodule so the decision logic in `mod.rs`
//! stays independent of the wire format.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command as ProcessCommand;

use serde::Deserialize;

use super::WorkspaceGraph;

/// Resolve the Cargo workspace graph via `cargo metadata --format-version 1`.
///
/// Returns `None` when the root has no Cargo manifest or Cargo cannot resolve
/// the workspace; the caller falls back to manifest discovery.
pub(super) fn cargo_metadata_graph(root: &Path) -> Option<WorkspaceGraph> {
	if !root.join("Cargo.toml").is_file() {
		return None;
	}

	let output = ProcessCommand::new("cargo")
		.current_dir(root)
		.args(["metadata", "--format-version", "1"])
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	let metadata: CargoMetadata = serde_json::from_slice(&output.stdout).ok()?;
	let members: BTreeSet<&str> = metadata
		.workspace_members
		.iter()
		.map(String::as_str)
		.collect();
	let names_by_id: BTreeMap<&str, &str> = metadata
		.packages
		.iter()
		.map(|package| (package.id.as_str(), package.name.as_str()))
		.collect();
	let mut graph = WorkspaceGraph::empty();

	for package in &metadata.packages {
		if !members.contains(package.id.as_str()) {
			continue;
		}

		let rel_dir = relative_manifest_dir(&package.manifest_path, &metadata.workspace_root);
		graph.insert_package(package.name.as_str(), rel_dir);
	}

	let resolve = metadata.resolve?;

	for node in &resolve.nodes {
		let Some(dependent) = names_by_id.get(node.id.as_str()) else {
			continue;
		};

		for dependency in &node.deps {
			let Some(dependency_name) = names_by_id.get(dependency.pkg.as_str()) else {
				continue;
			};

			if members.contains(node.id.as_str()) && members.contains(dependency.pkg.as_str()) {
				graph
					.deps
					.entry(dependent.to_string())
					.or_default()
					.insert(dependency_name.to_string());
			}
		}
	}

	Some(graph)
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
	resolve: Option<MetadataResolve>,
	workspace_root: String,
}

/// One `packages` entry from `cargo metadata`.
#[derive(Debug, Deserialize)]
struct MetadataPackage {
	id: String,
	name: String,
	manifest_path: String,
}

/// The resolved dependency graph from `cargo metadata`.
#[derive(Debug, Deserialize)]
struct MetadataResolve {
	#[serde(default)]
	nodes: Vec<MetadataNode>,
}

/// One `resolve.nodes` entry from `cargo metadata`.
#[derive(Debug, Deserialize)]
struct MetadataNode {
	id: String,
	#[serde(default)]
	deps: Vec<MetadataDependency>,
}

/// One resolved dependency edge; `pkg` already resolves renames.
#[derive(Debug, Deserialize)]
struct MetadataDependency {
	pkg: String,
}
