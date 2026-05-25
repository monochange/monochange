#![forbid(clippy::indexing_slicing)]

//! # `monochange_dart`
//!
//! <!-- {=monochangeDartCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_dart` discovers Dart and Flutter packages for the shared planner.
//!
//! Reach for this crate when you need to scan `pubspec.yaml` files, expand Dart or Flutter workspaces, and normalize package metadata into `monochange_core` records.
//!
//! ## Why use it?
//!
//! - cover both pure Dart and Flutter package layouts with one adapter
//! - normalize pubspec metadata and dependency edges for shared release planning
//! - detect Flutter packages without maintaining a separate discovery path
//!
//! ## Best for
//!
//! - scanning Dart or Flutter monorepos into normalized workspace records
//! - reusing the same planning pipeline for mobile and non-mobile packages
//! - discovering Flutter packages without a dedicated Flutter-only adapter layer
//!
//! ## Public entry points
//!
//! - `discover_dart_packages(root)` discovers Dart and Flutter workspaces plus standalone packages
//! - `DartAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - `pubspec.yaml` workspace expansion
//! - Dart package parsing
//! - Flutter package detection
//! - normalized dependency extraction
//! <!-- {/monochangeDartCrateDocs} -->

pub mod analysis;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub use analysis::DartSemanticAnalyzer;
pub use analysis::semantic_analyzer;
use glob::glob;
use monochange_core::AdapterDiscovery;
use monochange_core::DependencyKind;
use monochange_core::DependencySyncChange;
use monochange_core::DiscoveryPathFilter;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::LockfileCommandExecution;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDependency;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::ShellConfig;
use monochange_core::SourceConfiguration;
use monochange_core::VersionStrategy;
use monochange_core::normalize_path;
use monochange_publish::PublishRequest;
use semver::Version;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub mod lints;

pub const PUBSPEC_FILE: &str = "pubspec.yaml";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DartVersionedFileKind {
	Manifest,
	Lock,
}

pub fn write_dart_placeholder_manifest(
	dir: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	let repository =
		source.map(|source| format!("https://github.com/{}/{}", source.owner, source.repo));
	let mut rendered = format!(
		"name: {}\nversion: {}\ndescription: Placeholder package published by monochange.\n",
		request.package_name, request.version
	);
	if let Some(repository) = repository {
		let _ = writeln!(rendered, "repository: {repository}");
	}
	fs::write(dir.join("pubspec.yaml"), rendered).map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder pubspec.yaml: {error}"))
	})
}

/// Classify a Dart or Flutter versioned file path.
pub fn supported_versioned_file_kind(path: &Path) -> Option<DartVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		"pubspec.lock" => Some(DartVersionedFileKind::Lock),
		_ if path.extension().and_then(|ext| ext.to_str()) == Some("yaml")
			|| path.extension().and_then(|ext| ext.to_str()) == Some("yml") =>
		{
			Some(DartVersionedFileKind::Manifest)
		}
		_ => None,
	}
}

/// Discover lockfiles that should be refreshed for `package`.
pub fn discover_lockfiles(package: &PackageRecord) -> Vec<PathBuf> {
	let manifest_dir = package
		.manifest_path
		.parent()
		.map_or_else(|| package.workspace_root.clone(), Path::to_path_buf);
	let scope = if manifest_dir == package.workspace_root {
		manifest_dir.clone()
	} else {
		package.workspace_root.clone()
	};

	let mut discovered = [scope.join("pubspec.lock")]
		.into_iter()
		.filter(|path| path.exists())
		.collect::<Vec<_>>();

	if discovered.is_empty() && scope != manifest_dir {
		discovered.extend(
			[manifest_dir.join("pubspec.lock")]
				.into_iter()
				.filter(|path| path.exists()),
		);
	}

	discovered
}

/// Return the default lockfile refresh commands for `package`.
pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	if package.ecosystem != Ecosystem::Dart {
		return Vec::new();
	}

	let command = if package.metadata.contains_key("is_flutter") {
		"flutter pub get"
	} else {
		"dart pub get"
	};

	discover_lockfiles(package)
		.into_iter()
		.map(|lockfile| {
			LockfileCommandExecution {
				command: command.to_string(),
				cwd: lockfile
					.parent()
					.unwrap_or(&package.workspace_root)
					.to_path_buf(),
				shell: ShellConfig::None,
			}
		})
		.collect()
}

/// Update dependency sections inside a parsed `pubspec.yaml` mapping.
pub fn update_dependency_fields(
	mapping: &mut Mapping,
	fields: &[&str],
	versioned_deps: &BTreeMap<String, String>,
) {
	for field in fields {
		let Some(Value::Mapping(section)) = mapping.get_mut(Value::String(field.to_string()))
		else {
			continue;
		};

		for (dep_name, dep_version) in versioned_deps {
			let key = Value::String(dep_name.clone());

			if section.contains_key(&key) {
				section.insert(key, Value::String(dep_version.clone()));
			}
		}
	}
}

#[must_use = "the manifest update result must be checked"]
/// Update `pubspec.yaml` text while preserving the existing layout.
pub fn update_manifest_text(
	contents: &str,
	owner_version: Option<&str>,
	fields: &[&str],
	versioned_deps: &BTreeMap<String, String>,
) -> MonochangeResult<String> {
	serde_yaml_ng::from_str::<Mapping>(contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse pubspec yaml: {error}"))
	})?;

	let line_ranges = yaml_line_ranges(contents);
	let mut replacements = Vec::<((usize, usize), String)>::new();

	if let Some(owner_version) = owner_version
		&& let Some(span) = find_yaml_scalar_for_key(contents, &line_ranges, 0, "version")
	{
		replacements.push((
			span,
			render_yaml_scalar(&contents[span.0..span.1], owner_version),
		));
	}

	for field in fields {
		let Some(section_index) = find_yaml_key_line(contents, &line_ranges, 0, field) else {
			continue;
		};

		for (dep_name, dep_version) in versioned_deps {
			if let Some(span) =
				find_yaml_dependency_scalar(contents, &line_ranges, section_index, dep_name)
			{
				replacements.push((
					span,
					render_yaml_scalar(&contents[span.0..span.1], dep_version),
				));
			}
		}
	}

	replacements.sort_by_key(|right| std::cmp::Reverse(right.0.0));

	let mut updated = contents.to_string();
	for ((start, end), replacement) in replacements {
		updated.replace_range(start..end, &replacement);
	}

	Ok(updated)
}

fn yaml_line_ranges(contents: &str) -> Vec<(usize, usize)> {
	let mut ranges = Vec::new();
	let mut start = 0usize;
	for (index, ch) in contents.char_indices() {
		if ch == '\n' {
			ranges.push((start, index));
			start = index + 1;
		}
	}
	if start <= contents.len() {
		ranges.push((start, contents.len()));
	}
	ranges
}

fn find_yaml_scalar_for_key(
	contents: &str,
	line_ranges: &[(usize, usize)],
	indent: usize,
	key: &str,
) -> Option<(usize, usize)> {
	let line_index = find_yaml_key_line(contents, line_ranges, indent, key)?;
	let range = *line_ranges.get(line_index)?;
	parse_yaml_line(contents, range).and_then(|line| line.value_span)
}

fn find_yaml_key_line(
	contents: &str,
	line_ranges: &[(usize, usize)],
	indent: usize,
	key: &str,
) -> Option<usize> {
	line_ranges.iter().position(|range| {
		parse_yaml_line(contents, *range)
			.is_some_and(|line| line.indent == indent && line.key == key)
	})
}

fn find_yaml_dependency_scalar(
	contents: &str,
	line_ranges: &[(usize, usize)],
	section_index: usize,
	dep_name: &str,
) -> Option<(usize, usize)> {
	let section = parse_yaml_line(contents, *line_ranges.get(section_index)?)?;
	let section_indent = section.indent;
	let mut index = section_index + 1;
	while let Some(range) = line_ranges.get(index) {
		let Some(line) = parse_yaml_line(contents, *range) else {
			index += 1;
			continue;
		};
		if line.indent <= section_indent {
			break;
		}
		if line.key == dep_name {
			if let Some(value_span) = line.value_span {
				return Some(value_span);
			}
			let dep_indent = line.indent;
			let mut nested_index = index + 1;
			while let Some(nested_range) = line_ranges.get(nested_index) {
				let Some(nested_line) = parse_yaml_line(contents, *nested_range) else {
					nested_index += 1;
					continue;
				};
				if nested_line.indent <= dep_indent {
					break;
				}
				if nested_line.key == "version" {
					return nested_line.value_span;
				}
				nested_index += 1;
			}
			return None;
		}
		index += 1;
	}
	None
}

struct ParsedYamlLine<'a> {
	indent: usize,
	key: &'a str,
	value_span: Option<(usize, usize)>,
}

fn parse_yaml_line(contents: &str, range: (usize, usize)) -> Option<ParsedYamlLine<'_>> {
	let line = &contents[range.0..range.1];
	let trimmed = line.trim_start_matches([' ', '\t']);
	if trimmed.is_empty() || trimmed.starts_with('#') {
		return None;
	}
	let indent = line.len() - trimmed.len();
	let colon = trimmed.find(':')?;
	let key = trimmed[..colon].trim();
	if key.is_empty() {
		return None;
	}
	let value_span = yaml_value_span(line, range.0, indent + colon + 1);
	Some(ParsedYamlLine {
		indent,
		key,
		value_span,
	})
}

fn yaml_value_span(
	line: &str,
	line_start: usize,
	value_start_in_line: usize,
) -> Option<(usize, usize)> {
	let suffix = line.get(value_start_in_line..)?;
	let value_offset = suffix.find(|ch: char| !matches!(ch, ' ' | '\t'))?;
	let value = &suffix[value_offset..];
	if value.starts_with('#') {
		return None;
	}
	let span_start = line_start + value_start_in_line + value_offset;
	let span_end = if let Some(quote) = value
		.chars()
		.next()
		.filter(|quote| *quote == '"' || *quote == '\'')
	{
		let quote_end = find_yaml_quote_end(value, quote)?;
		span_start + quote_end + 1
	} else {
		let comment_index = value.find('#').unwrap_or(value.len());
		let trimmed_end = value[..comment_index].trim_end_matches([' ', '\t']).len();
		span_start + trimmed_end
	};
	(span_end > span_start).then_some((span_start, span_end))
}

fn find_yaml_quote_end(value: &str, quote: char) -> Option<usize> {
	let mut chars = value.char_indices();
	chars.next()?;
	for (index, ch) in chars {
		if ch == quote {
			return Some(index);
		}
	}
	None
}

fn render_yaml_scalar(existing: &str, value: &str) -> String {
	if existing.starts_with('"') && existing.ends_with('"') {
		return format!("\"{value}\"");
	}
	if existing.starts_with('\'') && existing.ends_with('\'') {
		return format!("'{value}'");
	}
	value.to_string()
}

/// Update versions embedded in a parsed `pubspec.lock` mapping.
pub fn update_pubspec_lock(mapping: &mut Mapping, raw_versions: &BTreeMap<String, String>) {
	let Some(Value::Mapping(packages)) = mapping.get_mut(Value::String("packages".to_string()))
	else {
		return;
	};
	for (name, version) in raw_versions {
		let key = Value::String(name.clone());
		let Some(Value::Mapping(entry)) = packages.get_mut(&key) else {
			continue;
		};
		entry.insert(
			Value::String("version".to_string()),
			Value::String(version.clone()),
		);
	}
}

pub struct DartAdapter;

/// Return the shared Dart and Flutter ecosystem adapter.
#[must_use]
pub const fn adapter() -> DartAdapter {
	DartAdapter
}

impl EcosystemAdapter for DartAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Dart
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_dart_packages(root)
	}

	fn load_configured(
		&self,
		root: &Path,
		package_path: &Path,
	) -> MonochangeResult<Option<PackageRecord>> {
		load_configured_dart_package(root, package_path)
	}

	fn supported_versioned_file_kind(&self, path: &Path) -> bool {
		supported_versioned_file_kind(path).is_some()
	}

	fn validate_versioned_file(
		&self,
		full_path: &Path,
		display_path: &str,
		custom_fields: Option<&[String]>,
	) -> MonochangeResult<()> {
		validate_versioned_file(full_path, display_path, custom_fields)
	}
}

#[tracing::instrument(skip_all)]
#[must_use = "the discovery result must be checked"]
/// Discover Dart and Flutter packages rooted at `root`.
pub fn discover_dart_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let workspace_manifests = find_workspace_manifests(root);
	let mut included_manifests = HashSet::new();
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for workspace_manifest in workspace_manifests {
		let (workspace_packages, workspace_warnings) =
			discover_workspace_packages(&workspace_manifest)?;
		warnings.extend(workspace_warnings);
		for package in workspace_packages {
			included_manifests.insert(package.manifest_path.clone());
			packages.push(package);
		}
	}

	for manifest_path in find_all_manifests(root) {
		if included_manifests.contains(&manifest_path) {
			continue;
		}

		if let Some(package) =
			parse_manifest(&manifest_path, manifest_path.parent().unwrap_or(root))?
		{
			packages.push(package);
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);
	tracing::debug!(packages = packages.len(), "discovered dart packages");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load one explicitly configured Dart/Flutter package without walking the repo.
#[must_use = "the package result must be checked"]
pub fn load_configured_dart_package(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let manifest_path =
		if package_path.file_name().and_then(|name| name.to_str()) == Some(PUBSPEC_FILE) {
			package_path.to_path_buf()
		} else {
			package_path.join(PUBSPEC_FILE)
		};
	parse_manifest(&manifest_path, manifest_path.parent().unwrap_or(root))
}

fn find_workspace_manifests(root: &Path) -> Vec<PathBuf> {
	let mut manifests = find_all_manifests(root)
		.into_iter()
		.filter(|manifest_path| has_workspace_section(manifest_path).unwrap_or(false))
		.collect::<Vec<_>>();
	manifests.sort();
	manifests
}

fn discover_workspace_packages(
	workspace_manifest: &Path,
) -> MonochangeResult<(Vec<PackageRecord>, Vec<String>)> {
	let parsed = parse_yaml_manifest(workspace_manifest)?;
	let workspace_root = workspace_manifest
		.parent()
		.unwrap_or_else(|| Path::new("."));
	let patterns = yaml_array_strings(&parsed, "workspace");
	let mut warnings = Vec::new();
	let manifests = expand_workspace_patterns(workspace_root, &patterns, &mut warnings);
	let mut packages = Vec::new();

	for manifest_path in manifests {
		if let Some(package) = parse_manifest(&manifest_path, workspace_root)? {
			packages.push(package);
		}
	}

	Ok((packages, warnings))
}

fn expand_workspace_patterns(
	root: &Path,
	patterns: &[String],
	warnings: &mut Vec<String>,
) -> BTreeSet<PathBuf> {
	let filter = DiscoveryPathFilter::new(root);
	let mut manifests = BTreeSet::new();
	for pattern in patterns {
		let joined_pattern = root.join(pattern).to_string_lossy().to_string();
		let matches = glob(&joined_pattern)
			.into_iter()
			.flat_map(|paths| paths.filter_map(Result::ok))
			.map(|path| normalize_path(&path))
			.filter(|path| filter.allows(path))
			.collect::<Vec<_>>();
		if matches.is_empty() {
			warnings.push(format!(
				"dart workspace pattern `{pattern}` under {} matched no packages",
				root.display()
			));
		}

		for matched_path in matches {
			let manifest_path = if matched_path.is_dir() {
				matched_path.join(PUBSPEC_FILE)
			} else {
				matched_path
			};
			if manifest_path.file_name().and_then(|name| name.to_str()) == Some(PUBSPEC_FILE)
				&& manifest_path.exists()
				&& filter.allows(&manifest_path)
			{
				manifests.insert(manifest_path);
			}
		}
	}
	manifests
}

fn parse_manifest(
	manifest_path: &Path,
	workspace_root: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let parsed = parse_yaml_manifest(manifest_path)?;
	let Some(name) = yaml_string(&parsed, "name") else {
		return Ok(None);
	};
	let is_flutter = parsed.get(Value::String("flutter".to_string())).is_some();
	let version = yaml_string(&parsed, "version").and_then(|value| Version::parse(&value).ok());
	let publish_state = manifest_publish_state(&parsed);

	let mut package = PackageRecord::new(
		Ecosystem::Dart,
		name,
		manifest_path.to_path_buf(),
		workspace_root.to_path_buf(),
		version,
		publish_state,
	);
	if is_flutter {
		package
			.metadata
			.insert("is_flutter".to_string(), "true".to_string());
	}
	package.declared_dependencies = parse_dependencies(&parsed);
	Ok(Some(package))
}

fn manifest_publish_state(parsed: &Mapping) -> PublishState {
	match parsed.get(Value::String("publish_to".to_string())) {
		Some(Value::String(value)) if value == "none" => PublishState::Private,
		Some(Value::Bool(false)) => PublishState::Private,
		_ => PublishState::Public,
	}
}

fn parse_dependencies(parsed: &Mapping) -> Vec<PackageDependency> {
	["dependencies", "dev_dependencies"]
		.into_iter()
		.filter_map(|section| {
			yaml_mapping(parsed, section).map(|dependencies| (section, dependencies))
		})
		.flat_map(|(section, dependencies)| {
			dependencies.iter().map(move |(name, value)| {
				PackageDependency {
					name: name.as_str().unwrap_or_default().to_string(),
					kind: DependencyKind::Runtime,
					version_constraint: match value {
						Value::String(text) => Some(text.clone()),
						Value::Mapping(mapping) => {
							mapping
								.get(Value::String("version".to_string()))
								.and_then(Value::as_str)
								.map(ToString::to_string)
						}
						_ => None,
					},
					optional: false,
					source_field: Some(section.to_string()),
				}
			})
		})
		.filter(|dependency| !dependency.name.is_empty())
		.collect()
}

fn has_workspace_section(manifest_path: &Path) -> MonochangeResult<bool> {
	let parsed = parse_yaml_manifest(manifest_path)?;
	Ok(parsed
		.get(Value::String("workspace".to_string()))
		.and_then(Value::as_sequence)
		.is_some_and(|items| !items.is_empty()))
}

fn parse_yaml_manifest(manifest_path: &Path) -> MonochangeResult<Mapping> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	serde_yaml_ng::from_str::<Mapping>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})
}

fn yaml_string(mapping: &Mapping, key: &str) -> Option<String> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_str)
		.map(ToString::to_string)
}

/// Return the default dependency-version prefix for this ecosystem.
/// Validate that a Dart versioned file contains a readable version field.
pub fn validate_versioned_file(
	full_path: &Path,
	display_path: &str,
	_custom_fields: Option<&[String]>,
) -> MonochangeResult<()> {
	let contents = fs::read_to_string(full_path).map_err(|error| {
		MonochangeError::Config(format!(
			"versioned file `{display_path}` is not readable: {error}"
		))
	})?;
	let yaml: Value = serde_yaml_ng::from_str(&contents).map_err(|error| {
		MonochangeError::Config(format!(
			"versioned file `{display_path}` is not valid YAML: {error}"
		))
	})?;

	if yaml
		.get("version")
		.and_then(|value| value.as_str())
		.is_none()
	{
		return Err(MonochangeError::Config(format!(
			"versioned file `{display_path}` does not contain a `version` string field"
		)));
	}

	Ok(())
}

#[must_use]
pub fn default_dependency_version_prefix() -> &'static str {
	"^"
}

/// Return the manifest fields that usually contain dependency versions.
#[must_use]
pub fn default_dependency_fields() -> &'static [&'static str] {
	&["dependencies", "dev_dependencies"]
}
/// Identify internal dependency references in a Dart pubspec.yaml that need
/// version synchronization and compute the target values.
///
/// This function scans `dependencies`, `dev_dependencies`, and
/// `dependency_overrides` for references to other workspace packages. For
/// each internal dependency found, it determines the correct version
/// constraint based on the `VersionStrategy` and `resolution: workspace`
/// setting.
///
/// # Arguments
///
/// * `contents` - The raw pubspec.yaml text.
/// * `version_map` - Map of `package_id` → canonical version.
/// * `workspace_package_names` - Set of all workspace package names.
/// * `strategy` - How to format version constraints.
///
/// # Errors
///
/// Returns an error if the YAML cannot be parsed.
pub fn sync_internal_dependency_versions(
	contents: &str,
	version_map: &BTreeMap<String, String>,
	workspace_package_names: &BTreeSet<String>,
	strategy: VersionStrategy,
) -> MonochangeResult<Vec<DependencySyncChange>> {
	let mapping: Mapping = serde_yaml_ng::from_str(contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse pubspec yaml for sync: {error}"))
	})?;

	let workspace_resolution = manifest_uses_workspace_resolution(&mapping);
	let prefix = version_prefix_for_strategy(strategy);

	let mut changes = Vec::new();

	for section in ["dependencies", "dev_dependencies", "dependency_overrides"] {
		let Some(deps) = yaml_mapping(&mapping, section) else {
			continue;
		};

		for (dep_key, dep_value) in deps {
			let Some(dep_name) = dep_key.as_str() else {
				continue;
			};

			if !workspace_package_names.contains(dep_name) {
				continue;
			}

			let Some(canonical_version) = version_map.get(dep_name) else {
				continue;
			};

			let new_constraint = format!("{prefix}{canonical_version}");

			if let Value::String(current_version) = dep_value {
				if current_version == &new_constraint {
					continue;
				}
				changes.push(DependencySyncChange {
					dependency_name: dep_name.to_string(),
					section: section.to_string(),
					old_value: current_value_to_string(dep_value),
					new_value: new_constraint,
				});
			} else if let Value::Mapping(detail) = dep_value {
				// Dependency is a mapping (e.g., with path:, hosted:, version: keys).
				if workspace_resolution {
					// With workspace resolution, convert path: deps to versioned.
					// The new dep should just be a version string.
					let old_representation = detail_to_string(detail);
					changes.push(DependencySyncChange {
						dependency_name: dep_name.to_string(),
						section: section.to_string(),
						old_value: old_representation,
						new_value: new_constraint,
					});
				} else {
					// Without workspace resolution, update the version: field within
					// the mapping if it exists.
					if let Some(Value::String(current_ver)) =
						detail.get(Value::String("version".to_string()))
						&& current_ver != new_constraint.as_str()
					{
						changes.push(DependencySyncChange {
							dependency_name: dep_name.to_string(),
							section: section.to_string(),
							old_value: current_ver.clone(),
							new_value: new_constraint,
						});
					}
				}
			}
		}
	}

	Ok(changes)
}

/// Determine the version constraint prefix for the given strategy.
fn version_prefix_for_strategy(strategy: VersionStrategy) -> &'static str {
	match strategy {
		VersionStrategy::Default | VersionStrategy::Caret => default_dependency_version_prefix(),
		VersionStrategy::Exact => "",
		VersionStrategy::Compatible => ">=",
	}
}

/// Extract a string representation of a dependency value for reporting.
fn current_value_to_string(value: &Value) -> String {
	match value {
		Value::String(s) => s.clone(),
		Value::Mapping(m) => detail_to_string(m),
		Value::Number(n) => n.to_string(),
		Value::Bool(b) => b.to_string(),
		Value::Null => "null".to_string(),
		Value::Sequence(_) => "[...]".to_string(),
		Value::Tagged(t) => current_value_to_string(&t.value),
	}
}

/// Convert a YAML mapping dependency detail to a readable string.
fn detail_to_string(detail: &Mapping) -> String {
	let mut parts: Vec<String> = detail
		.iter()
		.filter_map(|(k, v)| {
			let key = k.as_str()?;
			let val = match v {
				Value::String(s) => s.clone(),
				Value::Bool(b) => b.to_string(),
				Value::Number(n) => n.to_string(),
				Value::Null => "null".to_string(),
				_ => "[...]".to_string(),
			};
			Some(format!("{key}: {val}"))
		})
		.collect();
	parts.sort();
	parts.join(", ")
}

/// Check if a pubspec manifest declares `resolution: workspace`.
fn manifest_uses_workspace_resolution(mapping: &Mapping) -> bool {
	mapping
		.get(Value::String("resolution".to_string()))
		.and_then(Value::as_str)
		.is_some_and(|resolution| resolution.trim() == "workspace")
}

fn yaml_mapping<'map>(mapping: &'map Mapping, key: &str) -> Option<&'map Mapping> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_mapping)
}

fn yaml_array_strings(mapping: &Mapping, key: &str) -> Vec<String> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_sequence)
		.map(|items| {
			items
				.iter()
				.filter_map(Value::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default()
}

fn find_all_manifests(root: &Path) -> Vec<PathBuf> {
	let filter = DiscoveryPathFilter::new(root);
	WalkDir::new(root)
		.into_iter()
		.filter_entry(|entry| filter.should_descend(entry.path()))
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PUBSPEC_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

#[cfg(test)]
#[path = "__tests__/sync_tests.rs"]
mod sync_tests;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
