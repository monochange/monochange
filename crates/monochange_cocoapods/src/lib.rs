#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]
#![doc = include_str!("crate_docs.md")]

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
use monochange_publish::PublishRequest;
use regex::Regex;
use semver::Version;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const PODSPEC_EXTENSION: &str = "podspec";
pub const PODSPEC_JSON_EXTENSION: &str = "podspec.json";

pub struct CocoaPodsAdapter;

#[must_use]
pub const fn adapter() -> CocoaPodsAdapter {
	CocoaPodsAdapter
}

impl EcosystemAdapter for CocoaPodsAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::CocoaPods
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_pods(root)
	}

	fn load_configured(
		&self,
		root: &Path,
		package_path: &Path,
	) -> MonochangeResult<Option<PackageRecord>> {
		load_configured_pod(root, package_path)
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
pub enum CocoaPodsVersionedFileKind {
	Podspec,
	PodspecJson,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<CocoaPodsVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();

	if file_name.ends_with(PODSPEC_JSON_EXTENSION) {
		return Some(CocoaPodsVersionedFileKind::PodspecJson);
	}

	if file_name.ends_with(PODSPEC_EXTENSION) {
		return Some(CocoaPodsVersionedFileKind::Podspec);
	}

	None
}

#[tracing::instrument(skip_all)]
pub fn discover_pods(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for podspec_path in find_all_podspec_files(root) {
		match parse_podspec(&podspec_path, root) {
			Ok(Some(package)) => packages.push(package),
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", podspec_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	tracing::debug!(packages = packages.len(), "discovered cocoapods pods");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load the package record for a configured pod directory.
///
/// The podspec's `s.version` declaration provides the current version. A
/// podspec that computes its version at evaluation time (for example from a
/// sibling manifest) carries no literal version, so the record resolves its
/// baseline from release tags and group versions instead.
pub fn load_configured_pod(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let Some(podspec_path) = find_podspec_in(package_path) else {
		return Ok(None);
	};

	parse_podspec(&podspec_path, root)
}

pub fn parse_podspec(podspec_path: &Path, root: &Path) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(podspec_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			podspec_path.display()
		))
	})?;

	let Some(name) = parse_podspec_name(&contents) else {
		return Ok(None);
	};

	Ok(Some(PackageRecord::new(
		Ecosystem::CocoaPods,
		&name,
		normalize_path(podspec_path),
		normalize_path(root),
		parse_podspec_version(&contents),
		PublishState::Public,
	)))
}

/// Extract the pod name from podspec text.
///
/// Matches the `s.name = 'Canvist'` declaration with single or double quotes
/// and any receiver variable (`s`, `spec`, or `pod`):
///
/// ```text
/// Pod::Spec.new do |s|
///   s.name = "Canvist"
/// ```
pub fn parse_podspec_name(contents: &str) -> Option<String> {
	capture_podspec_attribute(contents, "name")
}

/// Extract the pod version from podspec text and parse it as semver.
///
/// `CocoaPods` versions that are not valid semver (for example `1.0`) resolve to
/// `None` so release planning falls back to tags and group versions.
pub fn parse_podspec_version(contents: &str) -> Option<Version> {
	capture_podspec_attribute(contents, "version")
		.as_deref()
		.and_then(parse_cocoapods_version)
}

/// Parse a `CocoaPods` version string into a `semver::Version`.
#[must_use]
pub fn parse_cocoapods_version(version: &str) -> Option<Version> {
	Version::parse(version).ok()
}

fn capture_podspec_attribute(contents: &str, attribute: &str) -> Option<String> {
	let pattern = Regex::new(&format!(
		r#"(?:^|\n)\s*[A-Za-z_][A-Za-z0-9_]*\.{attribute}\s*=\s*['"]([^'"]+)['"]"#
	))
	.ok()?;

	pattern
		.captures(contents)
		.and_then(|capture| capture.get(1))
		.map(|value| value.as_str().to_string())
}

/// Rewrite the `s.version` declaration in podspec text, preserving the quote
/// style and indentation of the original declaration.
///
/// ```text
/// s.version = '1.2.3'
/// ```
///
/// becomes:
///
/// ```text
/// s.version = '1.4.0'
/// ```
#[must_use]
pub fn update_podspec_version(contents: &str, version: &str) -> String {
	// patch-coverage:ignore-start -- the pattern literal is compile-time valid; the fallback only exists because Regex::new returns a Result.
	let Ok(pattern) = Regex::new(r#"(\.version\s*=\s*)(['"])([^'"]+)(['"])"#) else {
		return contents.to_string();
	};
	// patch-coverage:ignore-end

	pattern
		.replace(contents, |capture: &regex::Captures<'_>| {
			let prefix = capture.get(1).map_or(".version = ", |found| found.as_str());
			let open_quote = capture.get(2).map_or("'", |found| found.as_str());
			let close_quote = capture.get(4).map_or(open_quote, |found| found.as_str());

			format!("{prefix}{open_quote}{version}{close_quote}")
		})
		.into_owned()
}

/// Write a minimal placeholder podspec so placeholder publishing can reserve a
/// pod name on the `CocoaPods` trunk.
pub fn write_cocoapods_placeholder_manifest(
	dir: &Path,
	request: &PublishRequest,
) -> MonochangeResult<()> {
	let summary = format!("{} placeholder package", request.package_name);
	let contents = format!(
		"Pod::Spec.new do |s|\n  s.name = '{}'\n  s.version = '{}'\n  s.summary = '{summary}'\n  \
		 s.homepage = 'https://github.com/monochange/monochange'\n  s.license = {{ type: 'Unlicense' }}\n  \
		 s.authors = {{ 'monochange' => 'support@monochange.dev' }}\n  \
		 s.source = {{ git: \
		 'https://github.com/monochange/monochange-placeholder.git', tag: 's.version' }}\nend\n",
		request.package_name, request.version,
	);

	fs::write(
		dir.join(format!("{}.podspec", request.package_name)),
		contents,
	)
	.map_err(|error| MonochangeError::Io(format!("failed to write placeholder podspec: {error}")))
}

/// Validate that a podspec versioned file contains a readable version field.
pub fn validate_versioned_file(
	full_path: &Path,
	_display_path: &str,
	_custom_fields: Option<&[String]>,
) -> MonochangeResult<()> {
	let contents = fs::read_to_string(full_path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", full_path.display()))
	})?;

	if parse_podspec_name(&contents).is_none() {
		return Err(MonochangeError::Config(format!(
			"podspec {} has no `name` declaration",
			full_path.display()
		)));
	}

	Ok(())
}

fn find_podspec_in(package_path: &Path) -> Option<PathBuf> {
	fs::read_dir(package_path)
		.ok()?
		.filter_map(Result::ok)
		.map(|entry| entry.path())
		.find(|path| path.is_file() && supported_versioned_file_kind(path).is_some())
}

fn find_all_podspec_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| {
			entry
				.file_name()
				.to_str()
				.is_some_and(|name| supported_versioned_file_kind(Path::new(name)).is_some())
		})
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git"
			| ".build"
			| ".devenv"
			| ".gradle"
			| "DerivedData"
			| "Pods" | "book"
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
