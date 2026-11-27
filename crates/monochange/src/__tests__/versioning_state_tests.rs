#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::fs;

use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::versioning::LabelInputs;
use monochange_core::versioning::ReleaseTimestamp;
use monochange_core::versioning::ResetPolicy;
use monochange_core::versioning::StampBehaviour;
use monochange_core::versioning::ValueDefinition;
use tempfile::tempdir;

use crate::versioning_state::PackageValues;
use crate::versioning_state::ResolveContext;
use crate::versioning_state::ResolvedReleaseValues;
use crate::versioning_state::apply_counter_write_backs;
use crate::versioning_state::resolve_release_values;

fn value_definition(source: &str) -> ValueDefinition {
	toml::from_str(source).unwrap_or_else(|error| panic!("value definition should parse: {error}"))
}

fn package(id: &str, values: Vec<(&str, ValueDefinition)>) -> PackageDefinition {
	PackageDefinition {
		id: id.to_string(),
		path: std::path::PathBuf::from(id),
		package_type: PackageType::Npm,
		changelog: None,
		excluded_changelog_types: Vec::new(),
		bump_propagation: None,
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Namespaced,
		version_source: monochange_core::VersionSource::Manifest,
		initial_version: None,
		floating_tags: Vec::new(),
		bump_ceiling: None,
		classification_enforced: None,
		cli: None,
		values: values
			.into_iter()
			.map(|(id, definition)| (id.to_string(), definition))
			.collect(),
		display_version: None,
		publish: monochange_core::PublishSettings::default(),
	}
}

fn released(id: &str, version: &str) -> BTreeMap<String, String> {
	BTreeMap::from([(id.to_string(), version.to_string())])
}

struct Fixture {
	_dir: tempfile::TempDir,
	root: std::path::PathBuf,
}

fn fixture(counter_json: &str) -> Fixture {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path().to_path_buf();
	fs::write(root.join("build.json"), counter_json)
		.unwrap_or_else(|error| panic!("write counter: {error}"));
	Fixture { _dir: dir, root }
}

fn context<'a>(
	root: &'a std::path::Path,
	previous_version: Option<&'a str>,
	previous_inputs: Option<&'a LabelInputs>,
) -> ResolveContext<'a> {
	ResolveContext {
		root,
		timestamp: monochange_core::versioning::ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0)
			.unwrap_or_else(|error| panic!("timestamp: {error}")),
		previous_inputs,
		previous_version,
		commit: None,
		commit_timestamp: None,
	}
}

#[tokio::test]
async fn non_integer_counter_is_a_config_error() {
	let f = fixture("{\"build\": \"four\"}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("non-integer should fail");
	assert!(error.to_string().contains("non-negative integers"));
}

#[tokio::test]
async fn write_backs_rewrite_nested_counter_fields() {
	let f = fixture("{\"custom\": {\"build\": 4}}");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "custom.build".to_string(),
		value: 17,
	}];
	apply_counter_write_backs(&f.root, &write_backs)
		.await
		.unwrap_or_else(|error| panic!("write back: {error}"));
	let updated = fs::read_to_string(f.root.join("build.json"))
		.unwrap_or_else(|error| panic!("read: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&updated).unwrap_or_else(|error| panic!("parse: {error}"));
	assert_eq!(parsed["custom"]["build"], 17);
}

/// Build a context that has a commit available, for git-derived values.
fn context_with_commit<'a>(
	root: &'a std::path::Path,
	commit: &'a str,
	commit_timestamp: ReleaseTimestamp,
) -> ResolveContext<'a> {
	ResolveContext {
		root,
		timestamp: ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
		previous_inputs: None,
		previous_version: None,
		commit: Some(commit),
		commit_timestamp: Some(commit_timestamp),
	}
}

#[tokio::test]
async fn git_commit_count_reads_the_repository() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	init_git_repo(root);
	fs::write(root.join("tracked.txt"), "one")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	commit_all(root, "first");
	fs::write(root.join("tracked.txt"), "two")
		.unwrap_or_else(|error| panic!("rewrite tracked: {error}"));
	commit_all(root, "second");
	let head = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);

	let packages = vec![package(
		"app",
		vec![("rev", value_definition("git = \"commit_count\"\n"))],
	)];
	let context = context_with_commit(
		root,
		&head,
		ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
	);
	let resolved = resolve_release_values(
		&context,
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.packages["app"].values["rev"], "2");
	assert!(!resolved.packages["app"].monotonic);
}

fn init_git_repo(root: &std::path::Path) {
	git_in_temp_repo(root, &["init", "-b", "main"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
}

fn commit_all(root: &std::path::Path, message: &str) {
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", message]);
}

fn git_in_temp_repo(root: &std::path::Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_temp_repo(root: &std::path::Path, args: &[&str]) -> String {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}
