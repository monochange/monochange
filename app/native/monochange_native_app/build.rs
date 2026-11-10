fn main() {
	let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
	let dir = manifest.join("src/routes");
	println!("cargo:rerun-if-changed=src/routes");
	let generated = leptoskit_router_codegen::generate(&dir)
		.unwrap_or_else(|error| panic!("file routes failed to generate: {error}"));
	let output = manifest.join("src/routes.rs");
	leptoskit_router_codegen::write(&output, &generated)
		.unwrap_or_else(|error| panic!("file routes failed to write: {error}"));
	let status = std::process::Command::new("rustfmt")
		.arg("--edition")
		.arg("2024")
		.arg(&output)
		.status();

	if !matches!(status, Ok(status) if status.success()) {
		panic!("rustfmt failed for {}", output.display());
	}

	leptoskit_build::platforms().unwrap_or_else(|error| panic!("native platforms failed: {error}"));
}
