use std::ffi::OsString;

use insta::assert_json_snapshot;
use serde_json::Value;

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn snapshot_index_output_is_agent_readable() {
	let output = monochange::run_with_args_in_dir(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("snapshot"),
			OsString::from("--view"),
			OsString::from("index"),
		],
		repo_root(),
	)
	.await
	.unwrap_or_else(|error| panic!("snapshot command failed: {error}"));
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("snapshot output was not JSON: {error}\n{output}"));

	assert_json_snapshot!(value, {
		".tool.version" => "[version]",
	});
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn snapshot_subtree_light_output_is_agent_readable() {
	let output = monochange::run_with_args_in_dir(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("snapshot"),
			OsString::from("--view"),
			OsString::from("light"),
			OsString::from("step:discover"),
		],
		repo_root(),
	)
	.await
	.unwrap_or_else(|error| panic!("snapshot command failed: {error}"));
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("snapshot output was not JSON: {error}\n{output}"));

	assert_json_snapshot!(value, {
		".tool.version" => "[version]",
	});
}

fn repo_root() -> &'static std::path::Path {
	std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.and_then(std::path::Path::parent)
		.unwrap_or_else(|| panic!("integration test crate must live under crates/"))
}
