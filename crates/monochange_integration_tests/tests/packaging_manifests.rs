//! Validation tests for crate packaging manifests.
//!
//! Every published crate that embeds `src/crate_docs.md` through
//! `include_str!` must ship that file in its package tarball. A crate with an
//! explicit `package.include` list that omits the file packages a tarball
//! whose `lib.rs` cannot compile, which fails `cargo publish` verification —
//! discovered when the 0.11.0 crates.io rollout aborted on `monochange_core`.

use std::error::Error;
use std::path::Path;

use toml::Value;

const CRATE_DOC_INCLUDE: &str = "include_str!(\"crate_docs.md\")";

/// `package.include` patterns that cover `src/crate_docs.md`.
const COVERING_PATTERNS: [&str; 2] = ["src/crate_docs.md", "src/**/*.md"];

#[test]
fn crates_embedding_crate_docs_ship_the_file_in_their_package() -> Result<(), Box<dyn Error>> {
	let crates_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
	let mut checked = 0usize;
	let mut failures = Vec::new();

	for entry in std::fs::read_dir(&crates_root)? {
		let crate_root = entry?.path();
		let lib_rs = crate_root.join("src/lib.rs");
		if !lib_rs.exists() {
			continue;
		}

		let lib_source = std::fs::read_to_string(&lib_rs)?;
		if !lib_source.contains(CRATE_DOC_INCLUDE) {
			continue;
		}
		checked += 1;

		let crate_docs = crate_root.join("src/crate_docs.md");
		let crate_name = crate_root
			.file_name()
			.and_then(|name| name.to_str())
			.unwrap_or_default()
			.to_string();
		if !crate_docs.exists() {
			failures.push(format!(
				"{crate_name}: src/crate_docs.md must exist in git because src/lib.rs embeds it"
			));
			continue;
		}

		let manifest_text = std::fs::read_to_string(crate_root.join("Cargo.toml"))?;
		let manifest = toml::from_str::<Value>(&manifest_text)?;
		let Some(include) = manifest
			.get("package")
			.and_then(|package| package.get("include"))
			.and_then(Value::as_array)
		else {
			// Without an explicit include list, cargo packages every
			// git-tracked file, so the embedded docs ship by default.
			continue;
		};

		let patterns = include.iter().filter_map(Value::as_str).collect::<Vec<_>>();
		let covered = patterns
			.iter()
			.any(|pattern| COVERING_PATTERNS.contains(pattern));
		if !covered {
			failures.push(format!(
				"{crate_name}: package.include must list \"src/crate_docs.md\" (or \"src/**/*.md\") because src/lib.rs embeds it; current list: {patterns:?}"
			));
		}
	}

	assert!(
		checked >= 16,
		"expected the docs embedding in at least 16 crates, found {checked}"
	);
	assert!(
		failures.is_empty(),
		"crates whose publish tarball cannot compile:\n{}",
		failures.join("\n")
	);
	Ok(())
}
