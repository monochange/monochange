use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_config::load_workspace_configuration;
use monochange_core::ReleaseManifest;
use monochange_github::build_release_requests;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

const CASES: &[&str] = &[
	"group-empty-members-empty",
	"group-empty-one-member-note",
	"group-real-member-note",
	"ungrouped-empty-package",
];

fn fixture_path(case: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/group-release-note-fallback")
		.join(case)
}

fn setup_case(case: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(case), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "test"]);
	git(root, &["config", "user.email", "test@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	let output = Command::new("git")
		.current_dir(root)
		.env("GIT_AUTHOR_DATE", "2026-04-05T00:00:00Z")
		.env("GIT_COMMITTER_DATE", "2026-04-05T00:00:00Z")
		.args(["commit", "-m", "initial"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));
	assert!(
		output.status.success(),
		"git commit failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	tempdir
}

fn mc_json(root: &Path, args: &[&str]) -> Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {}: {error}", args.join(" ")));
	assert!(
		output.status.success(),
		"monochange {} failed\nstdout:\n{}\nstderr:\n{}",
		args.join(" "),
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!(
			"parse monochange json: {error}\nstdout:\n{}",
			String::from_utf8_lossy(&output.stdout)
		)
	})
}

fn prepare_release(root: &Path) -> Value {
	mc_json(root, &["step", "prepare-release", "--format", "json"])
}

fn publish_release_dry_run(root: &Path) -> Value {
	let prepare_output = prepare_release(root);
	let manifest: ReleaseManifest = serde_json::from_value(prepare_output.clone())
		.unwrap_or_else(|error| panic!("parse release manifest: {error}"));
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let source = configuration
		.source
		.as_ref()
		.unwrap_or_else(|| panic!("fixture did not configure a source"));
	let releases = build_release_requests(source, &manifest)
		.into_iter()
		.map(|release| {
			serde_json::json!({
				"targetId": release.target_id,
				"targetKind": release.target_kind,
				"tag_name": release.tag_name,
				"name": release.name,
				"body": release.body.unwrap_or_default(),
			})
		})
		.collect::<Vec<_>>();
	serde_json::json!({
		"manifest": prepare_output,
		"releases": releases,
	})
}

fn read_if_exists(root: &Path, relative: &str) -> Option<String> {
	let path = root.join(relative);
	path.exists().then(|| {
		std::fs::read_to_string(&path)
			.unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
	})
}

fn changelog_snapshots(root: &Path) -> Value {
	serde_json::json!({
		"group": read_if_exists(root, "changelog.md").is_some(),
		"core": read_if_exists(root, "crates/core/CHANGELOG.md").is_some(),
		"app": read_if_exists(root, "crates/app/CHANGELOG.md").is_some(),
	})
}

fn normalize_commit_links(contents: &str) -> String {
	contents
		.lines()
		.map(|line| {
			if line.contains("_Introduced in:_ [`") {
				"_Owner:_ test · _Introduced in:_ [`[commit]`](https://github.com/ifiokjr/monochange/commit/[commit])".to_string()
			} else {
				line.to_string()
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

fn release_bodies(publish_output: &Value) -> Vec<String> {
	publish_output["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("releases was not an array: {publish_output:#?}"))
		.iter()
		.map(|release| {
			release["body"]
				.as_str()
				.unwrap_or_else(|| panic!("release body was not a string: {release:#?}"))
				.to_string()
		})
		.collect()
}

fn release_summary(output: &Value) -> Value {
	let releases = output["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("releases was not an array: {output:#?}"))
		.iter()
		.map(|release| {
			serde_json::json!({
				"targetId": release["targetId"],
				"targetKind": release["targetKind"],
				"tag_name": release["tag_name"],
				"name": release["name"],
				"body": "[see string snapshot]",
			})
		})
		.collect::<Vec<_>>();
	serde_json::json!({
		"release_targets": output["manifest"]["release_targets"],
		"changelogs": output["manifest"]["changelogs"].as_array().unwrap_or_else(|| panic!("changelogs was not an array: {output:#?}")).iter().map(|changelog| {
			let sections = changelog["notes"]["sections"]
				.as_array()
				.unwrap_or_else(|| panic!("sections was not an array: {changelog:#?}"))
				.iter()
				.map(|section| {
					let entries = section["entries"]
						.as_array()
						.unwrap_or_else(|| panic!("entries was not an array: {section:#?}"))
						.iter()
						.map(|entry| {
							let entry = entry.as_str().unwrap_or_else(|| panic!("entry was not a string: {entry:#?}"));
							if entry.contains('\n') {
								Value::String("[multiline text]".to_string())
							} else {
								Value::String(normalize_commit_links(entry))
							}
						})
						.collect::<Vec<_>>();
					serde_json::json!({
						"collapsed": section["collapsed"],
						"entries": entries,
						"title": section["title"],
					})
				})
				.collect::<Vec<_>>();
			serde_json::json!({
				"owner_id": changelog["owner_id"],
				"owner_kind": changelog["owner_kind"],
				"path": changelog["path"],
				"sections": sections,
				"rendered": "[see string snapshot]",
			})
		}).collect::<Vec<_>>(),
		"releases": releases,
	})
}

#[test]
fn group_release_note_fallback_scenarios_snapshot_release_notes() {
	let mut snapshots = serde_json::Map::new();

	for case in CASES {
		let tempdir = setup_case(case);
		let output = publish_release_dry_run(tempdir.path());
		let bodies = release_bodies(&output);
		for (index, body) in bodies.iter().enumerate() {
			assert_snapshot!(
				format!("release_body__{case}__{index}"),
				normalize_commit_links(body)
			);
		}
		snapshots.insert((*case).to_string(), release_summary(&output));
	}

	assert_json_snapshot!(Value::Object(snapshots));
}

#[test]
fn group_release_note_fallback_scenarios_snapshot_generated_changelogs() {
	let mut snapshots = serde_json::Map::new();

	for case in CASES {
		let tempdir = setup_case(case);
		let output = prepare_release(tempdir.path());
		let root = tempdir.path();
		for (name, relative) in [
			("group", "changelog.md"),
			("core", "crates/core/CHANGELOG.md"),
			("app", "crates/app/CHANGELOG.md"),
		] {
			if let Some(contents) = read_if_exists(root, relative) {
				assert_snapshot!(
					format!("changelog__{case}__{name}"),
					normalize_commit_links(&contents)
				);
			}
		}
		snapshots.insert(
			(*case).to_string(),
			serde_json::json!({
				"release_targets": output["release_targets"],
				"changed_files": output["changed_files"],
				"changelogFiles": changelog_snapshots(root),
			}),
		);
	}

	assert_json_snapshot!(Value::Object(snapshots));
}
