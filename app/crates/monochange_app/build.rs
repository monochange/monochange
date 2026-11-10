//! Regenerates `src/routes.rs` from `src/routes/` before the crate compiles.

use std::path::Path;

fn main() {
	let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).to_path_buf();
	let dir = manifest.join("src/routes");
	println!("cargo:rerun-if-changed=src/routes");
	watch(&dir);

	let generated = leptoskit_router_codegen::generate(&dir)
		.unwrap_or_else(|error| panic!("file routes failed to generate: {error}"));
	let output = manifest.join("src/routes.rs");
	leptoskit_router_codegen::write(&output, &generated)
		.unwrap_or_else(|error| panic!("file routes failed to write: {error}"));
	rustfmt(&output);
}

fn rustfmt(path: &Path) {
	let status = std::process::Command::new("rustfmt")
		.arg("--edition")
		.arg("2024")
		.arg(path)
		.status();

	if !matches!(status, Ok(status) if status.success()) {
		panic!("rustfmt failed for {}", path.display());
	}
}

fn watch(dir: &Path) {
	let entries = match std::fs::read_dir(dir) {
		Ok(entries) => entries,
		Err(_) => return,
	};

	for entry in entries.flatten() {
		let path = entry.path();
		println!("cargo:rerun-if-changed={}", path.display());

		if path.is_dir() {
			watch(&path);
		}
	}
}
