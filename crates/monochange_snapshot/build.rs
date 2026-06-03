fn main() {
	let package_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|error| {
		panic!("Cargo provides CARGO_PKG_VERSION to build scripts: {error}")
	});
	let mut components = package_version.split('.');
	let major = components
		.next()
		.unwrap_or_else(|| panic!("Cargo package versions include a major component"));
	let minor = components
		.next()
		.unwrap_or_else(|| panic!("Cargo package versions include a minor component"));
	let schema_version = if major == "0" && minor == "0" {
		"0.1".to_string()
	} else {
		format!("{major}.{minor}")
	};
	println!("cargo:rustc-env=MONOCHANGE_SNAPSHOT_SCHEMA_VERSION={schema_version}");
}
