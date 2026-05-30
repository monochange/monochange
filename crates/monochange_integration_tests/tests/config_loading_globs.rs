use std::ffi::OsString;
use std::path::Path;

use monochange_test_helpers::copy_directory;
use tempfile::tempdir;

#[allow(clippy::disallowed_methods)]
#[tokio::test(flavor = "multi_thread")]
async fn inherited_ecosystem_globs_load_quick_repo_fixture() {
	let workspace = setup_fixture("config/inherited-ecosystem-globs");
	let root = workspace.path();

	let help = monochange::run_with_args_in_dir(
		"mc",
		[OsString::from("mc"), OsString::from("--help")],
		root,
	)
	.await
	.unwrap_or_else(|error| panic!("root help: {error}"));
	assert!(help.contains("Deploy fixture packages"));

	let configuration = monochange_config::load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let packages = configuration
		.packages
		.iter()
		.map(|package| {
			let inherited_globs = package
				.versioned_files
				.iter()
				.filter(|file| file.path.contains("**"))
				.count();
			serde_json::json!({
				"id": package.id,
				"type": format!("{:?}", package.package_type),
				"inherited_globs": inherited_globs,
			})
		})
		.collect::<Vec<_>>();

	insta::assert_json_snapshot!(packages);
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}
