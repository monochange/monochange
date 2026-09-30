//! Release planning must update the same Poetry version that discovery reads.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_test_helpers::setup_fixture;
use serde_json::Value;

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn prepare_release_updates_poetry_version_and_preserves_metadata() {
	let fixture = setup_fixture!("python/poetry-version-update/workspace");
	let plan = run_cli(fixture.path(), true).await;
	let planned: Value =
		serde_json::from_str(&plan).unwrap_or_else(|error| panic!("parse plan: {error}"));
	assert_eq!(planned["plan"]["decisions"][0]["planned_version"], "3.1.1");
	insta::assert_json_snapshot!(planned["plan"]);
	let dry_run_manifest = fs::read_to_string(fixture.path().join("pyproject.toml"))
		.unwrap_or_else(|error| panic!("read dry-run manifest: {error}"));
	let dry_run_parsed: toml::Value = toml::from_str(&dry_run_manifest)
		.unwrap_or_else(|error| panic!("parse dry-run manifest: {error}"));
	assert_eq!(
		dry_run_parsed["tool"]["poetry"]["version"].as_str(),
		Some("3.1.0")
	);

	let prepared = run_cli(fixture.path(), false).await;
	let release: Value = serde_json::from_str(&prepared)
		.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
	assert_eq!(release["plan"]["decisions"][0]["planned_version"], "3.1.1");
	let manifest = fs::read_to_string(fixture.path().join("pyproject.toml"))
		.unwrap_or_else(|error| panic!("read prepared manifest: {error}"));
	let parsed: toml::Value = toml::from_str(&manifest)
		.unwrap_or_else(|error| panic!("parse prepared manifest: {error}"));
	assert_eq!(parsed["tool"]["poetry"]["version"].as_str(), Some("3.1.1"));
	insta::assert_snapshot!(manifest);
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn prepare_release_updates_internal_poetry_dependency_constraints() {
	let fixture = setup_fixture!("python/poetry-dependency-release/workspace");
	let prepared = run_cli(fixture.path(), false).await;
	let release: Value = serde_json::from_str(&prepared)
		.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
	insta::assert_json_snapshot!(release["plan"]);
	let manifest = fs::read_to_string(fixture.path().join("cli/pyproject.toml"))
		.unwrap_or_else(|error| panic!("read prepared manifest: {error}"));
	let parsed: toml::Value = toml::from_str(&manifest)
		.unwrap_or_else(|error| panic!("parse prepared manifest: {error}"));
	assert_eq!(
		parsed["tool"]["poetry"]["dependencies"]["py-core"]["version"].as_str(),
		Some(">=1.1.0")
	);
	assert_eq!(
		parsed["tool"]["poetry"]["group"]["dev"]["dependencies"]["py-core"].as_str(),
		Some(">=1.1.0")
	);
	insta::assert_snapshot!(manifest);
}

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn prepare_release_normalizes_native_python_producer_names_for_both_manifest_formats() {
	let fixture = setup_fixture!("python/poetry-producer-name-normalization/workspace");
	let prepared = run_cli(fixture.path(), false).await;
	let release: Value = serde_json::from_str(&prepared)
		.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
	let versions = release["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("release decisions missing"))
		.iter()
		.map(|decision| {
			(
				decision["package"].as_str(),
				decision["planned_version"].as_str(),
				decision["trigger"].as_str(),
			)
		})
		.collect::<Vec<_>>();

	assert_eq!(
		versions,
		vec![
			(
				Some("python:core/pyproject.toml"),
				Some("1.1.0"),
				Some("direct-change")
			),
			(
				Some("python:pep621/pyproject.toml"),
				Some("2.0.1"),
				Some("transitive-dependency")
			),
			(
				Some("python:poetry/pyproject.toml"),
				Some("2.0.1"),
				Some("transitive-dependency")
			),
		]
	);
	insta::assert_json_snapshot!(release["plan"]);
	let poetry_manifest = fs::read_to_string(fixture.path().join("poetry/pyproject.toml"))
		.unwrap_or_else(|error| panic!("read prepared Poetry manifest: {error}"));
	let poetry: toml::Value = toml::from_str(&poetry_manifest)
		.unwrap_or_else(|error| panic!("parse prepared Poetry manifest: {error}"));
	assert_eq!(
		poetry["tool"]["poetry"]["dependencies"]["py-core"].as_str(),
		Some(">=1.1.0")
	);
	insta::assert_snapshot!(poetry_manifest);
	let pep621_manifest = fs::read_to_string(fixture.path().join("pep621/pyproject.toml"))
		.unwrap_or_else(|error| panic!("read prepared PEP 621 manifest: {error}"));
	let pep621: toml::Value = toml::from_str(&pep621_manifest)
		.unwrap_or_else(|error| panic!("parse prepared PEP 621 manifest: {error}"));
	assert_eq!(
		pep621["project"]["dependencies"][0].as_str(),
		Some("py_core>=1.1.0")
	);
	insta::assert_snapshot!(pep621_manifest);
}

async fn run_cli(root: &Path, dry_run: bool) -> String {
	let mut args = ["monochange", "step", "prepare-release", "--format", "json"]
		.map(OsString::from)
		.to_vec();

	if dry_run {
		args.push(OsString::from("--dry-run"));
	}

	monochange::run_with_args_in_dir("monochange", args, root)
		.await
		.unwrap_or_else(|error| panic!("monochange step prepare-release: {error}"))
}
