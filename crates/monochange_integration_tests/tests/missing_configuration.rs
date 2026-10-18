//! Integration tests for release commands run without a `monochange.toml`.

use std::ffi::OsString;
use std::path::Path;

use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_fixture() -> TempDir {
	let source =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/missing-configuration");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

#[test]
fn next_without_configuration_plans_with_built_in_defaults() {
	let fixture = setup_fixture();
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));
	let output = runtime
		.block_on(monochange::run_with_args_in_dir(
			"monochange",
			["monochange", "next", "--format", "json"].map(OsString::from),
			fixture.path(),
		))
		.unwrap_or_else(|error| panic!("next without monochange.toml: {error}"));

	insta::assert_snapshot!(output);
}
