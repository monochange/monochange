//! Initialization must fail before creating configuration that cannot be validated.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_test_helpers::setup_fixture;

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn init_rejects_shared_package_paths_before_writing_configuration_or_workflows() {
	let fixture = setup_fixture!("monochange/init-shared-package-path/workspace");
	let root = fixture.path();

	for (scenario, args) in [
		("without_provider", vec!["init"]),
		("github_provider", vec!["init", "--provider", "github"]),
	] {
		let error = run_cli(root, &args)
			.await
			.err()
			.unwrap_or_else(|| panic!("shared package paths must be rejected"));
		insta::assert_snapshot!(scenario, error.to_string());
		assert!(!root.join("monochange.toml").exists());
		assert!(!root.join(".github").exists());
	}
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn forced_init_rejects_shared_package_paths_without_overwriting_configuration() {
	let fixture = setup_fixture!("monochange/init-shared-package-path/workspace");
	let root = fixture.path();
	let config = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/monochange/populate-partial-cli/monochange.toml");
	let config = fs::read_to_string(config)
		.unwrap_or_else(|error| panic!("read existing config fixture: {error}"));
	fs::write(root.join("monochange.toml"), &config)
		.unwrap_or_else(|error| panic!("copy config fixture: {error}"));

	let error = run_cli(root, &["init", "--force", "--provider", "github"])
		.await
		.err()
		.unwrap_or_else(|| panic!("shared package paths must be rejected"));
	insta::assert_snapshot!(error.to_string());
	assert_eq!(
		fs::read_to_string(root.join("monochange.toml"))
			.unwrap_or_else(|error| panic!("read preserved config: {error}")),
		config
	);
	assert!(!root.join(".github").exists());
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn populate_explains_empty_defaults_and_preserves_custom_workflows() {
	let fixture = setup_fixture!("monochange/populate-partial-cli");
	let root = fixture.path();
	let before = fs::read_to_string(root.join("monochange.toml"))
		.unwrap_or_else(|error| panic!("read original config: {error}"));
	let output = run_cli(root, &["populate"])
		.await
		.unwrap_or_else(|error| panic!("populate output: {error}"));
	let output = output.replace(&root.display().to_string(), "[workspace]");

	insta::assert_snapshot!(output);
	assert_eq!(
		fs::read_to_string(root.join("monochange.toml"))
			.unwrap_or_else(|error| panic!("read preserved config: {error}")),
		before
	);
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn init_assigns_unique_ids_for_three_packages_with_the_same_name() {
	let fixture = setup_fixture!("monochange/init-triple-package-name/workspace");
	assert_valid_package_ids(fixture.path(), "triple_same_name").await;
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn init_generated_ids_do_not_shadow_native_package_names() {
	let fixture = setup_fixture!("monochange/init-package-id-collision/workspace");
	assert_valid_package_ids(fixture.path(), "native_name_collisions").await;
}

async fn assert_valid_package_ids(root: &Path, snapshot: &str) {
	run_cli(root, &["init"])
		.await
		.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(root.join("monochange.toml"))
		.unwrap_or_else(|error| panic!("read generated config: {error}"));
	let config: toml::Value =
		toml::from_str(&config).unwrap_or_else(|error| panic!("parse generated config: {error}"));

	insta::assert_json_snapshot!(snapshot, config["package"]);
	run_cli(root, &["step", "validate"])
		.await
		.unwrap_or_else(|error| panic!("validate generated config: {error}"));
}

async fn run_cli(root: &Path, args: &[&str]) -> monochange_core::MonochangeResult<String> {
	let args = std::iter::once("monochange")
		.chain(args.iter().copied())
		.map(OsString::from)
		.collect::<Vec<_>>();

	monochange::run_with_args_in_dir("monochange", args, root).await
}
