//! Regenerates `src/routes.rs` from `src/routes/` before the crate compiles.

use std::path::Path;

fn main() {
	let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).to_path_buf();
	let dir = manifest.join("src/routes");
	println!("cargo:rerun-if-changed=src/routes");
	watch(&dir);

	let mut generated = leptoskit_router_codegen::generate(&dir)
		.unwrap_or_else(|error| panic!("file routes failed to generate: {error}"));
	generated
		.source
		.push_str("\n#[cfg(test)]\n#[path = \"__tests__/routes_tests.rs\"]\nmod tests;\n");
	let output = manifest.join("src/routes.rs");
	leptoskit_router_codegen::write(&output, &generated)
		.unwrap_or_else(|error| panic!("file routes failed to write: {error}"));
	rustfmt(&output);
}

fn rustfmt(path: &Path) {
	// dprint is the repository formatter. rustfmt disagrees with it on the
	// generated match table, so prefer dprint when the build runs inside devenv.
	let dprint = std::process::Command::new("dprint")
		.arg("fmt")
		.arg(path)
		.status();
	if matches!(dprint, Ok(status) if status.success()) {
		return;
	}

	let status = std::process::Command::new("rustfmt")
		.arg("--edition")
		.arg("2024")
		.arg(path)
		.status();

	assert!(
		matches!(status, Ok(status) if status.success()),
		"rustfmt failed for {}",
		path.display()
	);
}

fn watch(dir: &Path) {
	let Ok(entries) = std::fs::read_dir(dir) else {
		return;
	};

	for entry in entries.flatten() {
		let path = entry.path();
		println!("cargo:rerun-if-changed={}", path.display());

		if path.is_dir() {
			watch(&path);
		}
	}
}
