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
async fn max_bump_caps_cli_snapshot_classification() {
	let fixture = monochange_test_helpers::setup_fixture!("cli-snapshot/max-bump");
	let output = monochange::run_with_args_in_dir(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("classify"),
			OsString::from("--cli-snapshot-before"),
			fixture.path().join("before.json").into_os_string(),
			OsString::from("--cli-snapshot-after"),
			fixture.path().join("after.json").into_os_string(),
			OsString::from("--format"),
			OsString::from("json"),
		],
		fixture.path(),
	)
	.await
	.unwrap_or_else(|error| panic!("classification command failed: {error}"));
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("classification output was not JSON: {error}\n{output}"));

	assert_json_snapshot!(value);
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn subcommand_snapshot_arg_renders_nested_subtree() {
	let fixture = monochange_test_helpers::setup_fixture!("cli-snapshot/minimal-workspace");
	let snapshot_arg_output = monochange::run_with_args_in_dir(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
			OsString::from("--snapshot"),
		],
		fixture.path(),
	)
	.await
	.unwrap_or_else(|error| panic!("subcommand snapshot failed: {error}"));
	let snapshot_command_output = run_snapshot(fixture.path(), ["migrate", "audit"]).await;
	assert_eq!(snapshot_arg_output, snapshot_command_output);

	let value: Value = serde_json::from_str(&snapshot_arg_output).unwrap_or_else(|error| {
		panic!("subcommand snapshot output was not JSON: {error}\n{snapshot_arg_output}")
	});
	assert_eq!(
		value["commands"][0]["path"],
		serde_json::json!(["migrate", "audit"])
	);
	assert!(
		value["commands"][0]
			.get("commands")
			.and_then(Value::as_array)
			.is_none_or(Vec::is_empty)
	);
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn snapshot_subtree_light_output_is_agent_readable() {
	let fixture = monochange_test_helpers::setup_fixture!("cli-snapshot/minimal-workspace");
	let output = run_snapshot(fixture.path(), ["--view", "light", "step", "discover"]).await;
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
