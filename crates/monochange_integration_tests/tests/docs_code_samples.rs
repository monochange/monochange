//! Validation tests for documentation code samples.
//!
//! Every fenced `toml` sample in the mdBook guide pages that represents a
//! `monochange.toml` configuration is parsed through the real configuration
//! loader so doc samples cannot drift from the supported schema.

use std::error::Error;
use std::path::Path;
use std::path::PathBuf;

use monochange_config::load_workspace_configuration;
use regex::Regex;
use tempfile::tempdir;

/// Directories whose samples document `monochange.toml` configuration.
const DOC_DIRS: [&str; 2] = ["docs/src/guide", "docs/src/reference"];

#[test]
fn guide_toml_samples_parse_through_the_real_loader() -> Result<(), Box<dyn Error>> {
	let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let mut checked = 0usize;
	let mut skipped = Vec::new();

	for doc_dir in DOC_DIRS {
		let dir = workspace_root.join(doc_dir);
		for entry in markdown_files(&dir)? {
			let text = std::fs::read_to_string(&entry)?;
			for sample in extract_toml_samples(&text) {
				if !looks_like_workspace_config(&sample.body) {
					skipped.push(format!("{}#{}", doc_dir, sample.index));
					continue;
				}
				write_sample_and_load(&sample.body)?;
				checked += 1;
			}
		}
	}

	assert!(
		checked >= 10,
		"expected the docs to carry at least 10 runnable monochange.toml samples, found {checked}"
	);
	Ok(())
}

/// Load a sample through the real configuration loader in a tempdir.
fn write_sample_and_load(sample: &str) -> Result<(), Box<dyn Error>> {
	let tempdir = tempdir()?;
	// Some snippets omit `[defaults].package_type` and shorthand versioned
	// file types on purpose; normalize both so the rest of the sample
	// validates. The loader accepts ecosystem shorthand on `type` fields.
	let needs_package_type = toml::from_str::<toml::Value>(sample)
		.ok()
		.and_then(|parsed| {
			let defaults_has_type = parsed
				.get("defaults")
				.and_then(|defaults| defaults.get("package_type"))
				.is_some();
			let packages = parsed.get("package").and_then(toml::Value::as_table)?;
			let any_missing_type = packages
				.values()
				.any(|package| package.get("type").is_none());
			Some(any_missing_type && !defaults_has_type)
		})
		.unwrap_or(false);
	let sample = if needs_package_type {
		format!("\n[defaults]\npackage_type = \"cargo\"\n\n{sample}",)
	} else {
		sample.to_string()
	};
	std::fs::write(tempdir.path().join("monochange.toml"), &sample)?;
	for capture in package_path_regex().captures_iter(&sample) {
		let relative = capture
			.get(1)
			.unwrap_or_else(|| panic!("package path capture"))
			.as_str();
		// Doc samples mix ecosystems; give each declared package the minimal
		// manifests its possible ecosystems expect so validation passes.
		let package_dir = tempdir.path().join(relative);
		std::fs::create_dir_all(&package_dir)?;
		for manifest in minimal_manifests() {
			let target = package_dir.join(manifest.0);
			if !target.exists() {
				std::fs::write(target, manifest.1)?;
			}
		}
	}
	load_workspace_configuration(tempdir.path())
		.map(|_| ())
		.map_err(|error| {
			let context = format!(
				"invalid doc sample (line {}):\n{sample}",
				sample.lines().count()
			);
			format!("{error}\n\n{context}")
		})?;
	Ok(())
}

/// Minimal manifests per supported ecosystem, keyed by file name.
fn minimal_manifests() -> Vec<(&'static str, &'static str)> {
	vec![
		(
			"Cargo.toml",
			"[package]\nname = \"docs-sample\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
		),
		(
			"package.json",
			"{ \"name\": \"docs-sample\", \"version\": \"0.1.0\" }",
		),
		(
			"pubspec.yaml",
			"name: docs_sample\nversion: 0.1.0\nenvironment:\n  sdk: ^3.0.0\n",
		),
		(
			"deno.json",
			"{ \"name\": \"docs-sample\", \"version\": \"0.1.0\" }",
		),
		("go.mod", "module example.com/docs-sample\n\ngo 1.21\n"),
		(
			"pyproject.toml",
			"[project]\nname = \"docs-sample\"\nversion = \"0.1.0\"\n",
		),
	]
}

fn package_path_regex() -> &'static Regex {
	static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
	RE.get_or_init(|| Regex::new(r#"(?m)^\s*path\s*=\s*"([^"]+)"\s*$"#).unwrap())
}

/// A sample is a fully-runnable workspace configuration when every declared
/// `[package.<id>]` table carries its `path`. Partial illustrative snippets
/// (which omit required fields on purpose) are skipped.
fn looks_like_workspace_config(sample: &str) -> bool {
	let parsed = match toml::from_str::<toml::Value>(sample) {
		Ok(value) => value,
		Err(_) => return false,
	};
	let Some(packages) = parsed.get("package").and_then(toml::Value::as_table) else {
		return false;
	};
	if packages.is_empty() {
		return false;
	}
	let all_packages_declared = packages.values().all(|package| {
		package
			.get("path")
			.and_then(toml::Value::as_str)
			.is_some_and(|path| !path.trim().is_empty())
	});
	// Samples that reference group members declared in other snippets are
	// fragments and cannot be validated standalone.
	let groups_reference_declared_packages = parsed
		.get("group")
		.and_then(toml::Value::as_table)
		.map(|groups| {
			groups.values().all(|group| {
				group
					.get("packages")
					.and_then(toml::Value::as_array)
					.map(|members| {
						members.iter().all(|member| {
							member.as_str().is_some_and(|id| packages.contains_key(id))
						})
					})
					.unwrap_or(true)
			})
		})
		.unwrap_or(true);
	all_packages_declared && groups_reference_declared_packages
}

struct TomlSample {
	body: String,
	index: usize,
}

/// Extract fenced ```toml blocks from a markdown document.
fn extract_toml_samples(text: &str) -> Vec<TomlSample> {
	let mut samples = Vec::new();
	let mut lines = text.lines().peekable();
	let mut index = 0usize;
	while let Some(line) = lines.next() {
		index += 1;
		if line.trim() != "```toml" {
			continue;
		}
		let mut body = Vec::new();
		let mut closed = false;
		for fence_line in lines.by_ref() {
			index += 1;
			if fence_line.trim() == "```" {
				closed = true;
				break;
			}
			body.push(fence_line);
		}
		if closed {
			samples.push(TomlSample {
				body: body.join("\n"),
				index,
			});
		}
	}
	samples
}

fn markdown_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	let mut files = Vec::new();
	let mut stack = vec![dir.to_path_buf()];
	while let Some(current) = stack.pop() {
		if current.is_file() {
			files.push(current);
			continue;
		}
		if !current.exists() {
			continue;
		}
		for entry in std::fs::read_dir(&current)? {
			let entry_path = entry?.path();
			if entry_path.is_dir() {
				stack.push(entry_path);
			} else if entry_path.extension().is_some_and(|ext| ext == "md") {
				files.push(entry_path);
			}
		}
	}
	files.sort();
	Ok(files)
}
