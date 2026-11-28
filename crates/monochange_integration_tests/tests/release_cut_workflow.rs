//! Frozen release cut workflow integration tests (`strategy = "cut"`):
//! - Cut branch resolution, status banner, and cut metadata markers
//! - Tag- and record-aware planning when cutting `vN+1` before `vN` syncs to `main`
//! - `SyncRelease` across `semantic_pr`, `bookkeeping_only`, and `merge_pr`
//! - Refusal to re-tag synced bookkeeping commits (`synced_from_commit`)
//! - Versionless `0.0.0-dev` main with `[defaults].version_source = "tag"`

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_fixture(relative: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(relative), root);
	git(root, &["init", "--initial-branch", "main"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	tempdir
}

fn run_monochange_raw(root: &Path, args: &[&str]) -> std::process::Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env_remove("GITHUB_TOKEN")
		.env_remove("GH_TOKEN")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-06-15")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {args:?}: {error}"))
}

fn run_monochange_json(root: &Path, args: &[&str]) -> Value {
	let output = run_monochange_raw(root, args);
	assert!(
		output.status.success(),
		"monochange {args:?} failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!(
			"parse monochange {args:?} json: {error}\nstdout:\n{}",
			String::from_utf8_lossy(&output.stdout)
		)
	})
}

fn git_rev(root: &Path, rev: &str) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(["rev-parse", rev])
		.output()
		.unwrap_or_else(|error| panic!("git rev-parse {rev}: {error}"));
	assert!(output.status.success());
	String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn normalize_commit_hashes(text: &str, shas: &[&str]) -> String {
	let mut normalized = text.to_string();
	for (index, sha) in shas.iter().enumerate() {
		let full_placeholder = format!("<commit_{index}>");
		let short_placeholder = format!("<sha_{index}>");
		normalized = normalized.replace(sha, &full_placeholder);
		if let Some(short) = sha.get(..7) {
			normalized = normalized.replace(short, &short_placeholder);
		}
	}
	normalized
		.lines()
		.map(|line| {
			if line.contains("_Introduced in:_") {
				"  - _Introduced in:_ <commit>"
			} else {
				line
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

#[test]
fn cut_strategy_validates_and_plans_frozen_cut_pull_request_branches() {
	let tempdir = setup_fixture("release-cut/basic");
	let root = tempdir.path();
	let base_sha = git_rev(root, "HEAD");

	let validate = run_monochange_raw(root, &["step", "validate"]);
	assert!(
		validate.status.success(),
		"validate failed:\n{}",
		String::from_utf8_lossy(&validate.stderr)
	);

	let output = run_monochange_json(root, &["run", "release", "--dry-run", "--format", "json"]);
	let request = &output["release_request"];
	let body = request["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected release_request.body string"));

	assert_json_snapshot!(
		"cut_dry_run_request_metadata",
		json!({
			"version": output["manifest"]["version"],
			"head_branch": request["head_branch"],
			"base_branch": request["base_branch"],
			"title": request["title"],
			"labels": request["labels"],
			"has_status_banner": body.contains("<!-- monochange:cut-status:start -->"),
			"has_cut_meta": body.contains("<!-- monochange:cut-meta:start "),
			"commit_subject": output["release_commit"]["subject"],
		})
	);
	assert_snapshot!(
		"cut_dry_run_request_body",
		normalize_commit_hashes(body, &[&base_sha])
	);
}

#[test]
fn tag_and_record_aware_planning_cuts_next_release_before_syncing_main() {
	let tempdir = setup_fixture("release-cut/basic");
	let root = tempdir.path();

	// 1. Cut v1.1.0 on a parallel branch and tag it (simulating Cut PR merge).
	git(root, &["checkout", "-b", "monochange/release/v1.1.0"]);
	let first_prepare = run_monochange_json(root, &["step", "prepare-release", "--format", "json"]);
	assert_eq!(first_prepare["version"], json!("1.1.0"));
	let commit = run_monochange_raw(root, &["step", "commit-release"]);
	assert!(
		commit.status.success(),
		"commit-release failed:\n{}",
		String::from_utf8_lossy(&commit.stderr)
	);
	// Move frozen cut branch `release/v1.1.0` onto the release commit (fast-forward merge).
	git(root, &["checkout", "-B", "release/v1.1.0"]);
	let tag = run_monochange_raw(
		root,
		&["step", "tag-release", "--from", "HEAD", "--push=false"],
	);
	assert!(
		tag.status.success(),
		"tag-release failed:\n{}",
		String::from_utf8_lossy(&tag.stderr)
	);

	// 2. Return to `main` (which still has Cargo.toml 1.0.0 and `.changeset/feat-a.md`!)
	// and land a second feature changeset before `v1.1.0` syncs back to `main`.
	git(root, &["checkout", "main"]);
	std::fs::write(
		root.join(".changeset/feat-b.md"),
		"---\ncore: minor\n---\n\nAdded second cut feature.\n",
	)
	.unwrap_or_else(|error| panic!("write feat-b.md: {error}"));
	git(root, &["add", ".changeset/feat-b.md"]);
	git(root, &["commit", "-m", "feat: add second feature"]);

	// 3. Prepare the next cut from un-synced `main`.
	let second_prepare =
		run_monochange_json(root, &["step", "prepare-release", "--format", "json"]);
	assert_eq!(second_prepare["version"], json!("1.2.0"));

	// Stale `.changeset/feat-a.md` was ignored in planning and deleted on disk.
	assert!(!root.join(".changeset/feat-a.md").exists());
	assert!(!root.join(".changeset/feat-b.md").exists());

	let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("read CHANGELOG.md: {error}"));
	let idx_120 = changelog
		.find("1.2.0")
		.unwrap_or_else(|| panic!("expected 1.2.0 in changelog:\n{changelog}"));
	let idx_110 = changelog
		.find("1.1.0")
		.unwrap_or_else(|| panic!("expected 1.1.0 in changelog:\n{changelog}"));
	let idx_100 = changelog
		.find("1.0.0")
		.unwrap_or_else(|| panic!("expected 1.0.0 in changelog:\n{changelog}"));
	assert!(
		idx_120 < idx_110 && idx_110 < idx_100,
		"expected chronological order 1.2.0 -> 1.1.0 -> 1.0.0:\n{changelog}"
	);

	assert_json_snapshot!(
		"unsynced_second_cut_summary",
		json!({
			"version": second_prepare["version"],
			"deleted_changesets": second_prepare["deleted_changesets"],
			"changed_files": second_prepare["changed_files"],
		})
	);
}

#[test]
fn sync_release_replays_published_cut_and_prevents_duplicate_tagging() {
	let tempdir = setup_fixture("release-cut/basic");
	let root = tempdir.path();

	// Cut, commit, and tag v1.1.0 on release/v1.1.0.
	git(root, &["checkout", "-b", "monochange/release/v1.1.0"]);
	let _ = run_monochange_json(root, &["step", "prepare-release", "--format", "json"]);
	let commit = run_monochange_raw(root, &["step", "commit-release"]);
	assert!(commit.status.success());
	let rel_commit = git_rev(root, "HEAD");
	git(root, &["checkout", "-B", "release/v1.1.0"]);
	let tag = run_monochange_raw(
		root,
		&["step", "tag-release", "--from", "HEAD", "--push=false"],
	);
	assert!(tag.status.success());

	// Advance main independently, then run `step sync-release --format json`.
	git(root, &["checkout", "main"]);
	std::fs::write(root.join("src/extra.rs"), "pub fn extra() {}\n")
		.unwrap_or_else(|error| panic!("write extra.rs: {error}"));
	git(root, &["add", "src/extra.rs"]);
	git(root, &["commit", "-m", "feat: independent main work"]);

	let sync_report = run_monochange_json(root, &["step", "sync-release", "--format", "json"]);
	assert_eq!(sync_report["status"], json!("completed"));
	assert_eq!(sync_report["mode"], json!("semantic_pr"));
	assert_eq!(sync_report["synced_tags"], json!(["v1.1.0"]));
	assert!(!root.join(".changeset/feat-a.md").exists());
	let cargo_toml = std::fs::read_to_string(root.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"));
	assert!(cargo_toml.contains("version = \"1.1.0\""));

	// Commit the synced bookkeeping onto main and verify `tag-release` refuses to re-tag it!
	git(root, &["add", "-A"]);
	git(
		root,
		&["commit", "-m", "chore(release): sync v1.1.0 to main"],
	);

	let retag = run_monochange_raw(
		root,
		&["step", "tag-release", "--from", "HEAD", "--push=false"],
	);
	assert!(
		!retag.status.success(),
		"expected synced commit to be rejected by tag-release"
	);
	let stderr = String::from_utf8_lossy(&retag.stderr);
	assert!(
		stderr.contains("synced bookkeeping commit"),
		"unexpected tag-release stderr: {stderr}"
	);

	// Subsequent sync-release is a clean no-op (`up_to_date`).
	let second_sync = run_monochange_json(root, &["step", "sync-release", "--format", "json"]);
	assert_eq!(second_sync["status"], json!("up_to_date"));

	assert_json_snapshot!(
		"semantic_pr_sync_report",
		json!({
			"mode": sync_report["mode"],
			"status": sync_report["status"],
			"cut_id": sync_report["cut_id"],
			"synced_tags": sync_report["synced_tags"],
			"synced_commit_matches_release": sync_report["synced_commits"] == json!([rel_commit]),
			"deleted_changesets": sync_report["deleted_changesets"],
			"updated_changelogs": sync_report["updated_changelogs"],
			"updated_manifests": sync_report["updated_manifests"],
			"second_sync_status": second_sync["status"],
		})
	);
}

#[test]
fn versionless_main_uses_tag_version_source_and_bookkeeping_only_sync() {
	let tempdir = setup_fixture("release-cut/versionless");
	let root = tempdir.path();
	git(root, &["tag", "v1.0.0"]);

	// Cut v1.1.0 from versionless main (Cargo.toml is 0.0.0-dev, seeded from v1.0.0 tag).
	git(root, &["checkout", "-b", "monochange/release/v1.1.0"]);
	let prepared = run_monochange_json(root, &["step", "prepare-release", "--format", "json"]);
	assert_eq!(prepared["version"], json!("1.1.0"));
	let commit = run_monochange_raw(root, &["step", "commit-release"]);
	assert!(commit.status.success());
	git(root, &["checkout", "-B", "release/v1.1.0"]);
	let tag = run_monochange_raw(
		root,
		&["step", "tag-release", "--from", "HEAD", "--push=false"],
	);
	assert!(tag.status.success());

	// Switch back to main and run bookkeeping_only sync.
	git(root, &["checkout", "main"]);
	let sync_report = run_monochange_json(root, &["step", "sync-release", "--format", "json"]);
	assert_eq!(sync_report["mode"], json!("bookkeeping_only"));
	assert_eq!(sync_report["status"], json!("completed"));
	assert_eq!(sync_report["updated_manifests"], json!([]));
	assert!(!root.join(".changeset/feat-a.md").exists());

	let cargo_toml = std::fs::read_to_string(root.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"));
	assert!(
		cargo_toml.contains("version = \"0.0.0-dev\""),
		"versionless main Cargo.toml should stay at 0.0.0-dev:\n{cargo_toml}"
	);

	assert_json_snapshot!(
		"versionless_bookkeeping_sync_report",
		json!({
			"prepared_version": prepared["version"],
			"sync_mode": sync_report["mode"],
			"sync_status": sync_report["status"],
			"synced_tags": sync_report["synced_tags"],
			"deleted_changesets": sync_report["deleted_changesets"],
			"updated_changelogs": sync_report["updated_changelogs"],
			"updated_manifests": sync_report["updated_manifests"],
		})
	);
}

#[test]
fn cut_from_older_commit_excludes_newer_changesets_and_code_in_dry_run_and_live() {
	let tempdir = setup_fixture("release-cut/basic");
	let root = tempdir.path();
	let m1 = git_rev(root, "HEAD");

	// Land a newer commit M2 on main with a major changeset and newer code.
	std::fs::write(
		root.join(".changeset/breaking-b.md"),
		"---\ncore: major\n---\n\nBreaking change landed after M1.\n",
	)
	.unwrap_or_else(|error| panic!("write breaking-b.md: {error}"));
	std::fs::write(root.join("src/lib.rs"), "pub fn newer_m2() {}\n")
		.unwrap_or_else(|error| panic!("write src/lib.rs: {error}"));
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "feat!: newer commit M2"]);
	let m2 = git_rev(root, "HEAD");
	assert_ne!(m1, m2);

	// 1. Dry-run cutting from M1 must plan ONLY 1.1.0 from M1 and leave checkout at M2.
	let dry_plan = run_monochange_json(
		root,
		&[
			"step",
			"prepare-release",
			"--from",
			&m1,
			"--dry-run",
			"--format",
			"json",
		],
	);
	assert_eq!(dry_plan["version"], json!("1.1.0"));
	let dry_changeset_paths: Vec<&str> = dry_plan["changesets"]
		.as_array()
		.unwrap_or_else(|| panic!("expected changesets array"))
		.iter()
		.filter_map(|entry| entry["path"].as_str())
		.collect();
	assert_eq!(dry_changeset_paths, vec![".changeset/feat-a.md"]);
	assert_eq!(git_rev(root, "HEAD"), m2);
	assert!(
		std::fs::read_to_string(root.join("src/lib.rs"))
			.unwrap_or_else(|error| panic!("read src/lib.rs: {error}"))
			.contains("newer_m2")
	);

	// 2. Live prepare cutting from M1 must check out M1 so M2's code/changesets never enter.
	let live_plan = run_monochange_json(
		root,
		&["step", "prepare-release", "--from", &m1, "--format", "json"],
	);
	assert_eq!(live_plan["version"], json!("1.1.0"));
	assert_eq!(
		live_plan["deleted_changesets"],
		json!([".changeset/feat-a.md"])
	);
	assert_eq!(git_rev(root, "HEAD"), m1);
	assert!(
		!std::fs::read_to_string(root.join("src/lib.rs"))
			.unwrap_or_else(|error| panic!("read src/lib.rs: {error}"))
			.contains("newer_m2"),
		"newer M2 code must not exist in a cut prepared from M1"
	);
	assert!(!root.join(".changeset/breaking-b.md").exists());
}

#[test]
fn single_invocation_open_sync_request_accepts_from_and_mode() {
	let tempdir = setup_fixture("release-cut/basic");
	let root = tempdir.path();

	git(root, &["checkout", "-b", "monochange/release/v1.1.0"]);
	let _ = run_monochange_json(root, &["step", "prepare-release", "--format", "json"]);
	let commit = run_monochange_raw(root, &["step", "commit-release"]);
	assert!(commit.status.success());
	git(root, &["checkout", "-B", "release/v1.1.0"]);
	let tag = run_monochange_raw(
		root,
		&["step", "tag-release", "--from", "HEAD", "--push=false"],
	);
	assert!(tag.status.success());

	git(root, &["checkout", "main"]);
	let output = run_monochange_json(
		root,
		&[
			"step",
			"open-sync-request",
			"--from",
			"v1.1.0",
			"--mode",
			"bookkeeping_only",
			"--dry-run",
			"--format",
			"json",
		],
	);
	assert_eq!(output["sync"]["status"], json!("dry_run"));
	assert_eq!(output["sync"]["mode"], json!("bookkeeping_only"));
	assert_eq!(output["sync"]["synced_tags"], json!(["v1.1.0"]));
	assert_eq!(output["sync"]["updated_manifests"], json!([]));
	assert_eq!(
		output["release_request"]["head_branch"],
		json!("monochange/sync-release")
	);
	assert_eq!(output["release_request"]["base_branch"], json!("main"));
}
