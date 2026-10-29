use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_test_helpers::setup_fixture;
use tempfile::tempdir;

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn format_rewrites_configuration_in_canonical_order() {
	let fixture = setup_fixture!("monochange/format/workspace");
	let path = fixture.path().join("monochange.toml");
	let output = run_cli(fixture.path(), &["format"])
		.await
		.unwrap_or_else(|error| panic!("format output: {error}"));
	let formatted =
		fs::read_to_string(&path).unwrap_or_else(|error| panic!("read formatted config: {error}"));

	insta::assert_snapshot!("output", output);
	insta::assert_snapshot!("config", formatted);

	let again = run_cli(fixture.path(), &["format"])
		.await
		.unwrap_or_else(|error| panic!("second format output: {error}"));
	let check = run_cli(fixture.path(), &["format", "--check"])
		.await
		.unwrap_or_else(|error| panic!("format check output: {error}"));

	insta::assert_snapshot!("already_formatted_output", again);
	insta::assert_snapshot!("check_output", check);
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn format_check_rejects_unformatted_configuration() {
	let fixture = setup_fixture!("monochange/format/workspace");
	let path = fixture.path().join("monochange.toml");
	let before = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read config: {error}"));
	let error = run_cli(fixture.path(), &["format", "--check"])
		.await
		.err()
		.unwrap_or_else(|| panic!("unformatted config must fail --check"));
	let after = fs::read_to_string(&path)
		.unwrap_or_else(|error| panic!("read config after check: {error}"));

	insta::assert_snapshot!("error", error.to_string());
	assert_eq!(before, after);
}

#[cfg(unix)]
#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn format_reports_write_failures() {
	use std::os::unix::fs::PermissionsExt;

	let fixture = setup_fixture!("monochange/format/workspace");
	let path = fixture.path().join("monochange.toml");
	let mut permissions = fs::metadata(&path)
		.unwrap_or_else(|error| panic!("read config permissions: {error}"))
		.permissions();
	permissions.set_mode(0o444);
	fs::set_permissions(&path, permissions)
		.unwrap_or_else(|error| panic!("make config read-only: {error}"));

	let error = run_cli(fixture.path(), &["format"])
		.await
		.err()
		.unwrap_or_else(|| panic!("read-only configuration must fail"));
	let message = error
		.to_string()
		.replace(&fixture.path().display().to_string(), "[workspace]");

	insta::assert_snapshot!(message);
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn format_reports_a_missing_configuration() {
	let root = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = run_cli(root.path(), &["format"])
		.await
		.err()
		.unwrap_or_else(|| panic!("missing configuration must fail"));
	let message = error
		.to_string()
		.replace(&root.path().display().to_string(), "[workspace]");

	insta::assert_snapshot!(message);
}

async fn run_cli(root: &Path, args: &[&str]) -> monochange_core::MonochangeResult<String> {
	let args = std::iter::once("monochange")
		.chain(args.iter().copied())
		.map(OsString::from)
		.collect::<Vec<_>>();

	monochange::run_with_args_in_dir("monochange", args, root).await
}
