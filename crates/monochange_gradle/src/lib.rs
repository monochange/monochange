#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]
//! <!-- {=monochangeGradleCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_gradle` discovers Gradle projects from `gradle.properties` manifests for the shared planner and publishes their libraries to Maven Central.
//!
//! Reach for this crate when you need to scan Kotlin and Java libraries, keep `VERSION_NAME` in lockstep with the rest of a monorepo, and run `./gradlew publishAndReleaseToMavenCentral` as part of a planned release.
//!
//! ## Why use it?
//!
//! - discover publishable Gradle projects through the `VERSION_NAME` convention
//! - read Maven coordinates from `GROUP`/`POM_GROUP_ID` and `POM_ARTIFACT_ID` properties
//! - publish releases to Maven Central through the Sonatype Central Portal
//! - detect already-published versions from Maven Central metadata
//! - inspect the publishing task graph without executing tasks during dry runs
//!
//! ## Best for
//!
//! - scanning Android and JVM libraries into normalized workspace records
//! - versioning Kotlin bindings together with the native libraries they wrap
//! - releasing to Maven Central with the `com.vanniktech.maven.publish` task set
//!
//! ## Public entry points
//!
//! - `discover_gradle_projects(root)` discovers Gradle projects under a repository root
//! - `parse_gradle_properties(path, root)` parses one `gradle.properties` manifest into package metadata
//! - `parse_properties(contents)` parses Java properties text into an ordered map
//! - `maven_central_metadata_path(group_id, artifact_id)` renders the repository path for a coordinate
//!
//! ## Scope
//!
//! - `gradle.properties` `VERSION_NAME` parsing with properties-file quoting rules
//! - Maven coordinate resolution from Android publishing conventions
//! - `rootProject.name` fallback names from `settings.gradle` and `settings.gradle.kts`
//! - no trusted publishing: Maven Central authenticates through portal user tokens and GPG keys
//! <!-- {/monochangeGradleCrateDocs} -->
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::AdapterDiscovery;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::normalize_path;
use regex::Regex;
use semver::Version;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const GRADLE_PROPERTIES_FILE: &str = "gradle.properties";
pub const VERSION_NAME_KEY: &str = "VERSION_NAME";
pub const GROUP_KEY: &str = "GROUP";
pub const POM_GROUP_ID_KEY: &str = "POM_GROUP_ID";
pub const ARTIFACT_ID_KEY: &str = "POM_ARTIFACT_ID";

pub const METADATA_GROUP_ID_KEY: &str = "group_id";
pub const METADATA_ARTIFACT_ID_KEY: &str = "artifact_id";

pub struct GradleAdapter;

#[must_use]
pub const fn adapter() -> GradleAdapter {
	GradleAdapter
}

impl EcosystemAdapter for GradleAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Gradle
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_gradle_projects(root)
	}

	fn load_configured(
		&self,
		root: &Path,
		package_path: &Path,
	) -> MonochangeResult<Option<PackageRecord>> {
		load_configured_gradle_project(root, package_path)
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GradleVersionedFileKind {
	GradleProperties,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<GradleVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();

	match file_name {
		GRADLE_PROPERTIES_FILE => Some(GradleVersionedFileKind::GradleProperties),
		_ => None,
	}
}

#[tracing::instrument(skip_all)]
pub fn discover_gradle_projects(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for properties_path in find_all_gradle_properties_files(root) {
		match parse_gradle_properties(&properties_path, root) {
			Ok(Some(package)) => packages.push(package),
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", properties_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	tracing::debug!(packages = packages.len(), "discovered gradle projects");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load the package record for a configured Gradle project directory.
///
/// A project qualifies when its `gradle.properties` declares `VERSION_NAME`,
/// the publishing convention shared by the Android library templates and the
/// `com.vanniktech.maven-publish` setup monochange releases through.
pub fn load_configured_gradle_project(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let properties_path = package_path.join(GRADLE_PROPERTIES_FILE);

	if !properties_path.is_file() {
		return Ok(None);
	}

	parse_gradle_properties(&properties_path, root)
}

pub fn parse_gradle_properties(
	properties_path: &Path,
	root: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(properties_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			properties_path.display()
		))
	})?;

	let properties = parse_properties(&contents);

	// A gradle.properties without VERSION_NAME configures a build, not a
	// publishable library.
	if !properties.contains_key(VERSION_NAME_KEY) {
		return Ok(None);
	}

	let manifest_dir = properties_path.parent().unwrap_or_else(|| Path::new("."));
	let artifact_id = properties
		.get(ARTIFACT_ID_KEY)
		.cloned()
		.or_else(|| project_name_from_settings(manifest_dir))
		.unwrap_or_else(|| fallback_directory_name(manifest_dir));
	let group_id = properties
		.get(GROUP_KEY)
		.or_else(|| properties.get(POM_GROUP_ID_KEY))
		.cloned();

	// The registry name is the full Maven coordinate when the group is known:
	// `dev.canvist:canvist-android` is what Maven Central metadata is keyed by.
	let name = if group_id.as_ref().is_some_and(|group| !group.is_empty()) {
		format!("{}:{artifact_id}", group_id.clone().unwrap_or_default())
	} else {
		artifact_id.clone()
	};

	let mut record = PackageRecord::new(
		Ecosystem::Gradle,
		&name,
		normalize_path(properties_path),
		normalize_path(root),
		properties
			.get(VERSION_NAME_KEY)
			.and_then(|version| Version::parse(version).ok()),
		PublishState::Public,
	);

	if let Some(group_id) = group_id.filter(|group| !group.is_empty()) {
		record
			.metadata
			.insert(METADATA_GROUP_ID_KEY.to_string(), group_id);
	}

	record
		.metadata
		.insert(METADATA_ARTIFACT_ID_KEY.to_string(), artifact_id);

	Ok(Some(record))
}

/// Parse Java properties text into an ordered map.
///
/// Handles `key=value`, `key:value`, and whitespace-separated pairs, strips
/// surrounding quotes from values, and skips comment and blank lines.
#[must_use]
pub fn parse_properties(contents: &str) -> BTreeMap<String, String> {
	let mut properties = BTreeMap::new();

	for line in contents.lines() {
		let trimmed = line.trim();

		if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
			continue;
		}

		let Some((key, value)) = split_property_line(trimmed) else {
			continue;
		};

		properties.insert(key.to_string(), unquote_property_value(value).to_string());
	}

	properties
}

fn split_property_line(line: &str) -> Option<(&str, &str)> {
	let separator = line.find(|ch: char| ch == '=' || ch == ':' || ch.is_whitespace())?;
	let key = &line[..separator];
	let value = line[separator..]
		.trim_start()
		.trim_start_matches(['=', ':'])
		.trim();
	Some((key.trim(), value))
}

fn unquote_property_value(value: &str) -> &str {
	value
		.strip_prefix('"')
		.and_then(|rest| rest.strip_suffix('"'))
		.unwrap_or(value)
}

/// Read `rootProject.name` from a Gradle settings script.
///
/// ```kotlin
/// rootProject.name = "canvist-android"
/// ```
pub fn project_name_from_settings(manifest_dir: &Path) -> Option<String> {
	let settings = [
		manifest_dir.join("settings.gradle.kts"),
		manifest_dir.join("settings.gradle"),
	];

	for settings_path in settings {
		let Ok(contents) = fs::read_to_string(&settings_path) else {
			continue;
		};

		if let Some(name) = parse_root_project_name(&contents) {
			return Some(name);
		}
	}

	// patch-coverage:ignore-start -- the loop-exit None return carries a zero-count region attributed to the closing brace.
	None
	// patch-coverage:ignore-end
}

pub fn parse_root_project_name(contents: &str) -> Option<String> {
	let pattern = Regex::new(r#"rootProject\.name\s*=\s*["']([^"']+)["']"#).ok()?;

	pattern
		.captures(contents)
		.and_then(|capture| capture.get(1))
		.map(|name| name.as_str().to_string())
}

/// Render the Maven Central repository path for a group and artifact pair.
///
/// `dev.canvist` with artifact `canvist-android` becomes
/// `dev/canvist/canvist-android`, the path `repo1.maven.org` serves metadata
/// from.
#[must_use]
pub fn maven_central_metadata_path(group_id: &str, artifact_id: &str) -> String {
	format!(
		"{}/{artifact_id}",
		group_id.replace('.', "/"),
		artifact_id = artifact_id
	)
}

/// Validate that a Gradle versioned file declares the version property.
pub fn validate_versioned_file(
	full_path: &Path,
	_display_path: &str,
	custom_fields: Option<&[String]>,
) -> MonochangeResult<()> {
	let contents = fs::read_to_string(full_path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", full_path.display()))
	})?;

	let properties = parse_properties(&contents);
	let fields = custom_fields.map_or_else(
		|| vec![VERSION_NAME_KEY],
		|fields| fields.iter().map(String::as_str).collect(),
	);

	for field in fields {
		if !properties.contains_key(field) {
			return Err(MonochangeError::Config(format!(
				"gradle properties file {} has no `{field}` property",
				full_path.display()
			)));
		}
	}

	Ok(())
}

/// Rewrite version-bearing property keys in `gradle.properties` text.
///
/// Each listed key (for example `VERSION_NAME`) is rewritten to the new
/// version, preserving indentation, separator spelling, and line endings.
/// Unlisted keys and comment lines are left untouched.
#[must_use]
pub fn update_gradle_properties_text(contents: &str, fields: &[&str], version: &str) -> String {
	if fields.is_empty() {
		return contents.to_string();
	}

	let mut result = String::with_capacity(contents.len());

	for line in contents.split_inclusive('\n') {
		let trimmed = line.trim();

		if let Some((key, value)) = split_property_line(trimmed)
			&& fields.contains(&key)
		{
			let prefix_length = line.len() - line.trim_start().len() + trimmed.len() - value.len();
			let prefix = line
				.get(..prefix_length)
				.expect("property prefix ends at a character boundary");
			let ending = if line.ends_with("\r\n") {
				"\r\n"
			} else if line.ends_with('\n') {
				"\n"
			} else {
				""
			};

			result.push_str(prefix);
			result.push_str(version);
			result.push_str(ending);
			continue;
		}

		result.push_str(line);
	}

	result
}

fn fallback_directory_name(manifest_dir: &Path) -> String {
	manifest_dir
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or("gradle-project")
		.to_string()
}

fn find_all_gradle_properties_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == GRADLE_PROPERTIES_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git"
			| ".gradle"
			| ".devenv"
			| "build" | "book"
			| "node_modules"
			| "target"
			| "testdata"
			| "vendor"
	)
}

/// Return the default dependency-version prefix for this ecosystem.
#[must_use]
pub fn default_dependency_version_prefix() -> &'static str {
	""
}

/// Return the manifest fields that usually contain dependency versions.
#[must_use]
pub fn default_dependency_fields() -> &'static [&'static str] {
	&[]
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
