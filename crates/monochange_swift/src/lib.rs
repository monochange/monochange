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
use regex::Regex;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const PACKAGE_SWIFT_FILE: &str = "Package.swift";

pub struct SwiftAdapter;

#[must_use]
pub const fn adapter() -> SwiftAdapter {
	SwiftAdapter
}

impl EcosystemAdapter for SwiftAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Swift
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_swift_packages(root)
	}

	fn load_configured(
		&self,
		root: &Path,
		package_path: &Path,
	) -> MonochangeResult<Option<PackageRecord>> {
		load_configured_swift_package(root, package_path)
	}

	fn supported_versioned_file_kind(&self, path: &Path) -> bool {
		supported_versioned_file_kind(path).is_some()
	}

	fn validate_versioned_file(
		&self,
		_full_path: &Path,
		_display_path: &str,
		_custom_fields: Option<&[String]>,
	) -> MonochangeResult<()> {
		Ok(())
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SwiftVersionedFileKind {
	PackageSwift,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<SwiftVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();

	match file_name {
		PACKAGE_SWIFT_FILE => Some(SwiftVersionedFileKind::PackageSwift),
		_ => None,
	}
}

#[tracing::instrument(skip_all)]
pub fn discover_swift_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for package_swift_path in find_all_package_swift_files(root) {
		match parse_package_swift(&package_swift_path, root) {
			Ok(Some(package)) => packages.push(package),
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", package_swift_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	tracing::debug!(packages = packages.len(), "discovered swift packages");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load the package record for a configured Swift package directory.
///
/// Swift Package Manager manifests carry no version field: released versions
/// are identified by repository tags, so the returned record resolves its
/// baseline from release tags instead of `Package.swift`.
pub fn load_configured_swift_package(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let package_swift_path = package_path.join(PACKAGE_SWIFT_FILE);

	if !package_swift_path.is_file() {
		return Ok(None);
	}

	parse_package_swift(&package_swift_path, root)
}

pub fn parse_package_swift(
	package_swift_path: &Path,
	root: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(package_swift_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			package_swift_path.display()
		))
	})?;

	let Some(name) = parse_package_name(&contents) else {
		return Ok(None);
	};

	Ok(Some(PackageRecord::new(
		Ecosystem::Swift,
		&name,
		normalize_path(package_swift_path),
		normalize_path(root),
		// Swift Package Manager versions come from repository tags, not the
		// manifest.
		None,
		PublishState::Public,
	)))
}

/// Extract the package name from `Package.swift` text.
///
/// The name is the first `name: "…"` argument of the `Package(…)` initializer,
/// skipping commented lines so a `// name: "Example"` note does not shadow it.
///
/// ```text
/// let package = Package(name: "Canvist", platforms: [.iOS(.v17)])
/// ```
pub fn parse_package_name(contents: &str) -> Option<String> {
	let name_pattern = Regex::new(r#"\bname\s*:\s*"([^"]+)""#).ok()?;

	for line in contents.lines() {
		for capture in name_pattern.captures_iter(line) {
			let match_start = capture.get(0).map_or(0, |found| found.start());

			// A `//` earlier on the line comments the match out.
			if line[..match_start].contains("//") {
				continue;
			}

			return capture.get(1).map(|name| name.as_str().to_string());
		}
	}

	None
}

fn find_all_package_swift_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PACKAGE_SWIFT_FILE)
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
