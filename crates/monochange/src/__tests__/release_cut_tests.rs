#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::process::Command;

use monochange_core::ChangelogFormat;
use monochange_core::CommitMessage;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordTarget;
use monochange_core::VersionFormat;
use monochange_core::VersionSource;
use tempfile::tempdir;

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn collect_unsynced_releases_skips_merged_and_on_disk_synced_records() {
	let repo = init_git_repo();
	let root = repo.path();

	// Create a release branch from main and commit a tagged release record.
	let main_base = git_rev(root, "HEAD");
	run_git(root, &["checkout", "-b", "release/v1.1.0"]);
	let record = sample_release_record(
		"1.1.0",
		"v1.1.0",
		"2026-06-01T00:00:00Z",
		Some(&main_base),
		None,
		vec![PathBuf::from(".changeset/feat-a.md")],
		"## 1.1.0\n\n- Added feature A\n",
	);
	write_release_record_and_commit(root, &record, "chore(release): v1.1.0");
	let rel_commit = git_rev(root, "HEAD");
	run_git(root, &["tag", "v1.1.0"]);

	// Switch back to main and advance main so v1.1.0 is unmerged.
	run_git(root, &["checkout", "main"]);
	write_and_commit(root, "src/new.txt", "work", "feat: new work on main");

	let unsynced = collect_unsynced_published_releases(root, &["v1.1.0".to_string()])
		.await
		.unwrap_or_else(|error| panic!("collect unsynced: {error}"));
	assert_eq!(unsynced.len(), 1);
	assert_eq!(unsynced[0].tag_name, "v1.1.0");
	assert_eq!(unsynced[0].commit, rel_commit);

	let consumed = unsynced_consumed_changeset_paths(root, &unsynced);
	assert!(consumed.contains(&PathBuf::from(".changeset/feat-a.md")));

	let versioning = latest_unsynced_release_versioning(root, &["v1.1.0".to_string()])
		.await
		.unwrap_or_else(|| panic!("expected unsynced release versioning"));
	assert_eq!(versioning.1.as_deref(), Some("1.1.0"));
	assert_eq!(versioning.2, "2026-06-01T00:00:00Z");

	// Once a synced copy of the record is written on disk (semantic_pr sync),
	// the tag is no longer reported as unsynced.
	let mut synced = record.clone();
	synced.synced_from_commit = Some(rel_commit);
	let record_paths = ReleasePaths::from_record(root, &synced);
	std::fs::create_dir_all(
		record_paths
			.absolute
			.parent()
			.unwrap_or_else(|| panic!("parent")),
	)
	.unwrap_or_else(|error| panic!("mkdir: {error}"));
	std::fs::write(
		&record_paths.absolute,
		serde_json::to_string_pretty(&synced).unwrap_or_else(|error| panic!("json: {error}")),
	)
	.unwrap_or_else(|error| panic!("write synced record: {error}"));

	let after_sync = collect_unsynced_published_releases(root, &["v1.1.0".to_string()])
		.await
		.unwrap_or_else(|error| panic!("collect after sync: {error}"));
	assert!(after_sync.is_empty());
}

#[test]
fn carry_forward_unsynced_changelogs_preserves_order_and_deduplicates_versions() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	std::fs::write(
		root.join("CHANGELOG.md"),
		"# Changelog\n\n## 1.0.0\n\n- Initial\n",
	)
	.unwrap_or_else(|error| panic!("write changelog: {error}"));

	let unsynced = vec![PublishedReleaseRecord {
		tag_name: "v1.1.0".to_string(),
		commit: "abc1234567890".to_string(),
		record_path: PathBuf::from(".monochange/releases/a/release.json"),
		record: sample_release_record(
			"1.1.0",
			"v1.1.0",
			"2026-06-01T00:00:00Z",
			None,
			None,
			Vec::new(),
			"## 1.1.0\n\n- Unsynced release\n",
		),
	}];
	let new_section = "## 1.2.0\n\n- New release\n".to_string();
	let changelog_path = root.join("CHANGELOG.md");
	let changelog_updates = vec![monochange_changelog::ChangelogUpdate {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		output: "default".to_string(),
		stream: "default".to_string(),
		format: ChangelogFormat::Monochange,
		file: monochange_changelog::FileUpdate {
			path: changelog_path.clone(),
			content: format!("# Changelog\n\n{new_section}\n## 1.0.0\n\n- Initial\n").into_bytes(),
		},
		notes: monochange_core::ReleaseNotesDocument {
			title: "1.2.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: new_section,
	}];
	let mut file_updates = vec![FileUpdate {
		path: changelog_path.clone(),
		content: changelog_updates[0].file.content.clone(),
	}];

	carry_forward_unsynced_changelogs(root, &unsynced, &changelog_updates, &mut file_updates)
		.unwrap_or_else(|error| panic!("carry forward: {error}"));

	let text = String::from_utf8(file_updates[0].content.clone())
		.unwrap_or_else(|error| panic!("utf8: {error}"));
	let idx_120 = text
		.find("## 1.2.0")
		.unwrap_or_else(|| panic!("missing 1.2.0 in {text}"));
	let idx_110 = text
		.find("## 1.1.0")
		.unwrap_or_else(|| panic!("missing 1.1.0 in {text}"));
	let idx_100 = text
		.find("## 1.0.0")
		.unwrap_or_else(|| panic!("missing 1.0.0 in {text}"));
	assert!(
		idx_120 < idx_110 && idx_110 < idx_100,
		"unexpected order: {text}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn compute_cut_status_banner_counts_commits_and_filters_consumed_changesets() {
	let repo = init_git_repo();
	let root = repo.path();
	let base_commit = git_rev(root, "HEAD");

	write_and_commit(
		root,
		".changeset/consumed.md",
		"---\ncore: patch\n---\nconsumed\n",
		"chore: consumed changeset",
	);
	write_and_commit(
		root,
		".changeset/pending.md",
		"---\ncore: minor\n---\npending\n",
		"feat: pending changeset",
	);

	let metadata = CutMetadata {
		cut_id: "v1.1.0".to_string(),
		cut_base_branch: "release/v1.1.0".to_string(),
		cut_base_commit: base_commit,
		head_branch: "monochange/release/v1.1.0".to_string(),
		base_branch: "main".to_string(),
		release_commit: None,
		targets: vec!["core@1.1.0".to_string()],
	};
	let mut consumed = BTreeSet::new();
	consumed.insert(PathBuf::from(".changeset/consumed.md"));

	let banner = compute_cut_status_banner(root, &metadata, "HEAD", "/refresh", &consumed)
		.await
		.unwrap_or_else(|error| panic!("compute banner: {error}"));
	assert!(
		banner.contains("**2 newer commits** (1 changeset) on `main`"),
		"unexpected banner: {banner}"
	);
	assert!(banner.contains(".changeset/pending.md"));
	assert!(!banner.contains(".changeset/consumed.md"));
	assert!(banner.contains("`/refresh`"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_sync_release_supports_none_bookkeeping_semantic_and_merge_pr_bridge() {
	let repo = init_git_repo();
	let root = repo.path();

	write_and_commit(
		root,
		"monochange.toml",
		sample_monochange_toml(false),
		"chore: add monochange.toml",
	);
	write_and_commit(
		root,
		"Cargo.toml",
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
		"chore: add Cargo.toml",
	);
	write_and_commit(
		root,
		"src/lib.rs",
		"pub fn hello() {}\n",
		"chore: add src/lib.rs",
	);
	write_and_commit(
		root,
		"CHANGELOG.md",
		"# Changelog\n\n## 1.0.0\n\n- Initial\n",
		"docs: initial changelog",
	);
	write_and_commit(
		root,
		".changeset/feat-a.md",
		"---\ncore: minor\n---\nAdded feature A\n",
		"feat: add changeset",
	);
	let cut_base = git_rev(root, "HEAD");

	// Cut release v1.1.0 on a parallel branch and tag it.
	run_git(root, &["checkout", "-b", "release/v1.1.0"]);
	let record = sample_release_record(
		"1.1.0",
		"v1.1.0",
		"2026-06-01T00:00:00Z",
		Some(&cut_base),
		None,
		vec![PathBuf::from(".changeset/feat-a.md")],
		"## 1.1.0\n\n- Added feature A\n",
	);
	write_release_record_and_commit(root, &record, "chore(release): v1.1.0");
	let rel_commit = git_rev(root, "HEAD");
	run_git(root, &["tag", "v1.1.0"]);

	// Advance main independently with an unrelated file.
	run_git(root, &["checkout", "main"]);
	write_and_commit(
		root,
		"README.md",
		"updated on main\n",
		"docs: update readme",
	);

	let config = monochange_config::load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load config: {error}"));

	// 1. SyncMode::None skips cleanly.
	let skipped = execute_sync_release(root, &config, None, Some(SyncMode::None), false)
		.await
		.unwrap_or_else(|error| panic!("sync none: {error}"));
	assert_eq!(skipped.status, "skipped");
	assert!(skipped.changed_files.is_empty());

	// 2. Dry-run BookkeepingOnly reports planned edits without touching disk.
	let dry = execute_sync_release(root, &config, None, Some(SyncMode::BookkeepingOnly), true)
		.await
		.unwrap_or_else(|error| panic!("sync dry: {error}"));
	assert_eq!(dry.status, "dry_run");
	assert_eq!(
		dry.deleted_changesets,
		vec![PathBuf::from(".changeset/feat-a.md")]
	);
	assert_eq!(dry.updated_changelogs, vec![PathBuf::from("CHANGELOG.md")]);
	assert!(dry.updated_manifests.is_empty());
	assert!(root.join(".changeset/feat-a.md").is_file());

	// 3. Real MergePr sync updates manifests, deletes changesets, writes synced record,
	// and commits a two-parent bridge commit making `v1.1.0` an ancestor of `HEAD`.
	let merged = execute_sync_release(root, &config, None, Some(SyncMode::MergePr), false)
		.await
		.unwrap_or_else(|error| panic!("sync merge_pr: {error}"));
	assert_eq!(merged.status, "completed");
	assert_eq!(merged.synced_tags, vec!["v1.1.0".to_string()]);
	assert_eq!(merged.synced_commits, vec![rel_commit.clone()]);
	assert!(!root.join(".changeset/feat-a.md").exists());
	let cargo_toml = std::fs::read_to_string(root.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"));
	assert!(cargo_toml.contains("version = \"1.1.0\""));
	let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("read CHANGELOG.md: {error}"));
	assert!(changelog.contains("## 1.1.0"));

	let sync_request = build_sync_change_request(
		config.source.as_ref().unwrap_or_else(|| panic!("source")),
		&merged,
	);
	assert_eq!(sync_request.head_branch, "monochange/sync-release");
	assert_eq!(sync_request.base_branch, "main");
	assert!(sync_request.title.contains("v1.1.0"));
	assert!(sync_request.body.contains("merge_pr"));

	let text_report = render_sync_release_report(&merged, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("render text: {error}"));
	assert!(text_report.contains("Sync release (completed, mode: merge_pr): v1.1.0"));
	let json_report = render_sync_release_report(&merged, OutputFormat::Json)
		.unwrap_or_else(|error| panic!("render json: {error}"));
	assert!(json_report.contains("\"merge_pr\""));

	let bridge_sha = commit_sync_release_local(
		root,
		&merged,
		&CommitMessage {
			subject: "chore(release): sync v1.1.0".to_string(),
			body: None,
		},
		false,
		false,
	)
	.await
	.unwrap_or_else(|error| panic!("bridge commit: {error}"));
	assert!(!bridge_sha.is_empty());

	// Verify `v1.1.0` is now an ancestor of HEAD via the two-parent bridge commit!
	let status = Command::new("git")
		.current_dir(root)
		.args(["merge-base", "--is-ancestor", "v1.1.0", "HEAD"])
		.status()
		.unwrap_or_else(|error| panic!("merge-base: {error}"));
	assert!(
		status.success(),
		"expected v1.1.0 to be an ancestor of HEAD"
	);

	// Subsequent sync reports up_to_date.
	let up_to_date = execute_sync_release(root, &config, None, Some(SyncMode::SemanticPr), false)
		.await
		.unwrap_or_else(|error| panic!("sync up_to_date: {error}"));
	assert_eq!(up_to_date.status, "up_to_date");

	// Explicit --from pointing at the synced bookkeeping commit must fail.
	let synced_err = execute_sync_release(
		root,
		&config,
		Some("HEAD"),
		Some(SyncMode::SemanticPr),
		true,
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected synced commit error"));
	assert!(
		synced_err
			.to_string()
			.contains("already a synced bookkeeping record commit")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_sync_release_skips_tag_version_source_packages() {
	let repo = init_git_repo();
	let root = repo.path();
	write_and_commit(
		root,
		"monochange.toml",
		sample_monochange_toml(true),
		"chore: add monochange.toml",
	);
	write_and_commit(
		root,
		"Cargo.toml",
		"[package]\nname = \"core\"\nversion = \"0.0.0-dev\"\nedition = \"2021\"\n",
		"chore: add versionless Cargo.toml",
	);
	write_and_commit(
		root,
		"src/lib.rs",
		"pub fn hello() {}\n",
		"chore: add src/lib.rs",
	);

	run_git(root, &["checkout", "-b", "release/v1.0.0"]);
	let record = sample_release_record(
		"1.0.0",
		"v1.0.0",
		"2026-06-01T00:00:00Z",
		None,
		None,
		Vec::new(),
		"## 1.0.0\n\n- Initial release\n",
	);
	write_release_record_and_commit(root, &record, "chore(release): v1.0.0");
	run_git(root, &["tag", "v1.0.0"]);
	run_git(root, &["checkout", "main"]);

	let config = monochange_config::load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load config: {error}"));
	assert_eq!(config.defaults.version_source, Some(VersionSource::Tag));
	assert_eq!(config.packages[0].version_source, VersionSource::Tag);

	let report = execute_sync_release(root, &config, None, Some(SyncMode::SemanticPr), false)
		.await
		.unwrap_or_else(|error| panic!("sync: {error}"));
	assert!(report.updated_manifests.is_empty());
	let cargo_toml = std::fs::read_to_string(root.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"));
	assert!(cargo_toml.contains("version = \"0.0.0-dev\""));
}

fn sample_monochange_toml(tag_version_source: bool) -> &'static str {
	if tag_version_source {
		r#"[defaults]
version_source = "tag"

[source]
provider = "github"
owner = "monochange"
repo = "monochange"

[source.releases]
branches = ["release/*"]

[source.pull_requests]
strategy = "cut"
base = "main"

[package.core]
path = "."
type = "cargo"
version_format = "primary"
tag = true
release = true
"#
	} else {
		r#"[source]
provider = "github"
owner = "monochange"
repo = "monochange"

[source.releases]
branches = ["release/*"]

[source.pull_requests]
strategy = "cut"
base = "main"

[package.core]
path = "."
type = "cargo"
version_format = "primary"
tag = true
release = true
"#
	}
}

fn sample_release_record(
	version: &str,
	tag_name: &str,
	created_at: &str,
	cut_base_commit: Option<&str>,
	synced_from_commit: Option<&str>,
	deleted_changesets: Vec<PathBuf>,
	rendered_changelog: &str,
) -> ReleaseRecord {
	let mut versions = BTreeMap::new();
	versions.insert("core".to_string(), version.to_string());
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: created_at.to_string(),
		command: "release-pr".to_string(),
		version: Some(version.to_string()),
		versions,
		release_targets: vec![ReleaseRecordTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: version.to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: tag_name.to_string(),
			members: Vec::new(),
			rendered_title: tag_name.to_string(),
			rendered_changelog_title: version.to_string(),
			floating_tags: Vec::new(),
		}],
		released_packages: vec!["core".to_string()],
		changed_files: vec![PathBuf::from("Cargo.toml"), PathBuf::from("CHANGELOG.md")],
		package_publications: Vec::new(),
		updated_changelogs: vec![PathBuf::from("CHANGELOG.md")],
		deleted_changesets,
		changesets: Vec::new(),
		changelogs: vec![ReleaseManifestChangelog {
			owner_id: "core".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			output: "default".to_string(),
			stream: "default".to_string(),
			path: PathBuf::from("CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
			notes: monochange_core::ReleaseNotesDocument {
				title: version.to_string(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: rendered_changelog.to_string(),
		}],
		values: BTreeMap::new(),
		labels: BTreeMap::new(),
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		cut_base_commit: cut_base_commit.map(ToString::to_string),
		synced_from_commit: synced_from_commit.map(ToString::to_string),
		provider: None,
	}
}

fn write_release_record_and_commit(root: &Path, record: &ReleaseRecord, message: &str) {
	let paths = ReleasePaths::from_record(root, record);
	std::fs::create_dir_all(paths.absolute.parent().unwrap_or_else(|| panic!("parent")))
		.unwrap_or_else(|error| panic!("mkdir: {error}"));
	let json = serde_json::to_string_pretty(record)
		.unwrap_or_else(|error| panic!("serialize record: {error}"));
	std::fs::write(&paths.absolute, format!("{json}\n"))
		.unwrap_or_else(|error| panic!("write record: {error}"));
	run_git(root, &["add", "-A"]);
	run_git(root, &["commit", "-m", message]);
}

fn init_git_repo() -> tempfile::TempDir {
	let repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	run_git(repo.path(), &["init", "-b", "main"]);
	run_git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	run_git(repo.path(), &["config", "user.name", "monochange tests"]);
	run_git(repo.path(), &["config", "commit.gpgsign", "false"]);
	write_and_commit(repo.path(), "README.md", "root", "initial commit");
	repo
}

fn write_and_commit(root: &Path, path: &str, contents: &str, message: &str) {
	let full = root.join(path);
	if let Some(parent) = full.parent() {
		std::fs::create_dir_all(parent).unwrap_or_else(|error| panic!("mkdir: {error}"));
	}
	std::fs::write(full, contents).unwrap_or_else(|error| panic!("write {path}: {error}"));
	run_git(root, &["add", path]);
	run_git(root, &["commit", "-m", message]);
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

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
}
