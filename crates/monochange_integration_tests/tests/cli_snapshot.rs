use std::ffi::OsString;
use std::path::Path;

use insta::assert_json_snapshot;
use serde_json::Value;

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn snapshot_index_output_is_agent_readable() {
	let fixture = monochange_test_helpers::setup_fixture!("cli-snapshot/minimal-workspace");
	let output = run_snapshot(fixture.path(), ["--view", "index"]).await;
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("snapshot output was not JSON: {error}\n{output}"));

	assert_json_snapshot!(value, {
		".tool.version" => "[version]",
	});
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn snapshot_subtree_light_output_is_agent_readable() {
	let fixture = monochange_test_helpers::setup_fixture!("cli-snapshot/minimal-workspace");
	let output = run_snapshot(fixture.path(), ["--view", "light", "step:discover"]).await;
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("snapshot output was not JSON: {error}\n{output}"));

	assert_json_snapshot!(value, {
		".tool.version" => "[version]",
	});
}

async fn run_snapshot<const N: usize>(root: &Path, args: [&str; N]) -> String {
	let mut cli_args = vec![OsString::from("mc"), OsString::from("snapshot")];
	cli_args.extend(args.into_iter().map(OsString::from));

	monochange::run_with_args_in_dir("mc", cli_args, root)
		.await
		.unwrap_or_else(|error| panic!("snapshot command failed: {error}"))
}
