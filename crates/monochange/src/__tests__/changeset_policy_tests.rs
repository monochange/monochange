#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::SourceProvider as ProviderKind;
use monochange_core::VersionFormat;
use monochange_core::VersionSource;

use super::*;
use crate::tests::TEST_ENV_LOCK;

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn git(root: &Path, args: &[&str]) {
	let mut command = Command::new("git");
	command.current_dir(root);
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}
	let output = command
		.args(["-c", "commit.gpgsign=false"])
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_uses_global_affected_path_filters() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config = config.replace(
		"comment_on_failure = true",
		"comment_on_failure = true\nchanged_paths = [\"infra/**\"]\nignored_paths = [\"docs/**\"]",
	);
	config = config.replace(
		"path = \"crates/core\"\nignored_paths",
		"path = \"crates/core\"\nchangelog = \"crates/core/CHANGELOG.md\"\nignored_paths",
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let evaluation = affected_packages(
		fixture.path(),
		&[
			"docs/readme.md".to_string(),
			"crates/core/CHANGELOG.md".to_string(),
			"infra/config.yml".to_string(),
		],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("affected packages for global paths: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert_eq!(
		evaluation.ignored_paths,
		vec![
			"docs/readme.md".to_string(),
			"crates/core/CHANGELOG.md".to_string()
		]
	);
	assert_eq!(
		evaluation.matched_paths,
		vec!["infra/config.yml".to_string()]
	);
	assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_skips_release_pull_request_branches_on_detached_head() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		"\n[source]\nprovider = \"github\"\nowner = \"monochange\"\nrepo = \"monochange\"\n\n[source.pull_requests]\nbranch_prefix = \"monochange/release\"\n",
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	git(fixture.path(), &["init", "-b", "main"]);
	git(fixture.path(), &["config", "user.name", "monochange"]);
	git(
		fixture.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	git(fixture.path(), &["config", "commit.gpgsign", "false"]);
	git(fixture.path(), &["add", "."]);
	git(fixture.path(), &["commit", "-m", "initial"]);
	git(fixture.path(), &["checkout", "--detach"]);

	let _env_lock = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);
	let evaluation = crate::tests::block_on_in_context(temp_env::async_with_vars(
		[("GITHUB_HEAD_REF", Some("monochange/release/affected"))],
		async {
			affected_packages(
				fixture.path(),
				&["crates/core/src/lib.rs".to_string()],
				&Vec::new(),
			)
			.await
			.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"))
		},
	));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Skipped);
	assert!(!evaluation.required);
	assert_eq!(
		evaluation.summary,
		"changeset verification skipped because current branch `monochange/release/affected` starts with release pull request branch prefix `monochange/release`"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn evaluate_and_verify_changesets_delegate_to_affected_packages() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let changed_paths = vec!["crates/core/src/lib.rs".to_string()];
	let labels = vec!["no-changeset-required".to_string()];

	let affected = affected_packages(fixture.path(), &changed_paths, &labels)
		.await
		.unwrap_or_else(|error| panic!("affected packages: {error}"));
	let verified = verify_changesets(fixture.path(), &changed_paths, &labels)
		.await
		.unwrap_or_else(|error| panic!("verify changesets: {error}"));
	let evaluated = evaluate_changeset_policy(fixture.path(), &changed_paths, &labels)
		.await
		.unwrap_or_else(|error| panic!("evaluate changeset policy: {error}"));

	assert_eq!(verified, affected);
	assert_eq!(evaluated, affected);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_uses_invalid_changeset_summary_when_only_changeset_inputs_fail() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	fs::create_dir_all(fixture.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(fixture.path().join(".changeset/invalid.md"), "---\ncore:\n")
		.unwrap_or_else(|error| panic!("write invalid changeset: {error}"));

	let evaluation = affected_packages(
		fixture.path(),
		&[".changeset/invalid.md".to_string()],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert_eq!(
		evaluation.summary,
		"changeset verification failed: one or more attached changeset files are invalid"
	);
	assert!(
		evaluation
			.errors
			.iter()
			.any(|error| error.contains("failed to parse"))
	);
}
