#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use monochange_core::FloatingTagFormat;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::ReleaseRecordTarget;
use monochange_core::VersionFormat;
use tempfile::TempDir;

use super::*;
use crate::git_support;

fn git(repo: &Path, args: &[&str]) {
	let output = std::process::Command::new("git")
		.args(args)
		.current_dir(repo)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn path_to_str(path: &Path) -> &str {
	path.to_str()
		.unwrap_or_else(|| panic!("path is not valid utf8: {}", path.display()))
}

fn init_release_repo() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	git(root, &["init", "--initial-branch", "main"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["config", "commit.gpgsign", "false"]);
	fs::write(root.join("README.md"), "# test\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn floating_tags_record(
	version: &str,
	tag_name: &str,
	floating_tags: &[&str],
) -> ReleaseRecordDiscovery {
	let record = ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some(version.to_string()),
		versions: std::collections::BTreeMap::from([("actions".to_string(), version.to_string())]),
		release_targets: vec![ReleaseRecordTarget {
			id: "actions".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: version.to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: tag_name.to_string(),
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
			members: vec!["actions".to_string()],
			floating_tags: floating_tags
				.iter()
				.map(|template| FloatingTagFormat((*template).to_string()))
				.collect(),
		}],
		released_packages: vec!["actions".to_string()],
		changed_files: Vec::new(),
		package_publications: Vec::new(),
		updated_changelogs: Vec::new(),
		changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		changesets: Vec::new(),
		provider: None,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	};

	ReleaseRecordDiscovery {
		input_ref: "HEAD".to_string(),
		resolved_commit: String::new(),
		record_commit: String::new(),
		distance: 0,
		record,
	}
}

async fn resolved_discovery(
	root: &Path,
	mut discovery: ReleaseRecordDiscovery,
) -> ReleaseRecordDiscovery {
	let commit = git_support::resolve_git_commit_ref(root, "HEAD")
		.await
		.unwrap_or_else(|error| panic!("resolve HEAD: {error}"));
	discovery.record_commit = commit.clone();
	discovery.resolved_commit = commit;
	discovery
}

#[tokio::test(flavor = "multi_thread")]
async fn create_release_tags_creates_and_moves_floating_tags() {
	let fixture = init_release_repo();
	let root = fixture.path();
	let discovery = floating_tags_record(
		"1.0.1",
		"v1.0.1",
		&["v{{ major }}.{{ minor }}", "v{{ major }}"],
	);
	let discovery = resolved_discovery(root, discovery).await;
	let record_commit = discovery.record_commit.clone();

	let report = create_release_tags(root, &discovery, false, false)
		.await
		.unwrap_or_else(|error| panic!("create release tags: {error}"));

	assert_eq!(report.status, "completed");
	assert_eq!(report.tag_results.len(), 1);
	let result = &report.tag_results[0];
	assert_eq!(result.tag_name, "v1.0.1");
	assert_eq!(result.floating_results.len(), 2);
	assert_eq!(result.floating_results[0].tag_name, "v1.0");
	assert_eq!(result.floating_results[1].tag_name, "v1");

	for tag in ["v1.0.1", "v1.0", "v1"] {
		let commit = git_support::resolve_git_commit_ref(root, tag)
			.await
			.unwrap_or_else(|error| panic!("resolve {tag}: {error}"));
		assert_eq!(commit, record_commit, "tag {tag} at release commit");
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn create_release_tags_reports_existing_floating_tag_targets() {
	let fixture = init_release_repo();
	let root = fixture.path();
	git(root, &["tag", "v1.0", "HEAD"]);
	git(root, &["tag", "v1", "HEAD"]);
	let discovery = floating_tags_record(
		"1.0.1",
		"v1.0.1",
		&["v{{ major }}.{{ minor }}", "v{{ major }}"],
	);
	let discovery = resolved_discovery(root, discovery).await;

	let report = create_release_tags(root, &discovery, false, false)
		.await
		.unwrap_or_else(|error| panic!("create release tags: {error}"));

	let result = &report.tag_results[0];
	assert_eq!(
		result.floating_results[0].previous_commit.as_deref(),
		Some(discovery.record_commit.as_str())
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_release_tags_skips_aliases_matching_the_release_tag() {
	let fixture = init_release_repo();
	let root = fixture.path();
	let mut discovery = floating_tags_record(
		"1.0.1",
		"release-1.0.1",
		&["release-{{ major }}.{{ minor }}.{{ patch }}"],
	);
	discovery.record.release_targets[0].version_format =
		VersionFormat::Custom("release-{{ version }}".to_string());
	let discovery = resolved_discovery(root, discovery).await;

	let report = create_release_tags(root, &discovery, false, false)
		.await
		.unwrap_or_else(|error| panic!("create release tags: {error}"));

	assert_eq!(report.tag_results[0].floating_results.len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_release_tags_rejects_non_semver_target_versions() {
	let fixture = init_release_repo();
	let root = fixture.path();
	let discovery = floating_tags_record("not-a-version", "v1.0.1", &["v{{ major }}"]);
	let discovery = resolved_discovery(root, discovery).await;

	let error = create_release_tags(root, &discovery, false, false)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected error"));

	assert!(error.to_string().contains("non-semver version"));
}

#[tokio::test(flavor = "multi_thread")]
async fn create_release_tags_rejects_invalid_floating_templates() {
	let fixture = init_release_repo();
	let root = fixture.path();
	let discovery = floating_tags_record("1.0.1", "v1.0.1", &["v{{ beta }}"]);
	let discovery = resolved_discovery(root, discovery).await;

	let error = create_release_tags(root, &discovery, false, false)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected error"));

	assert!(
		error
			.to_string()
			.contains("invalid `floating_tags` template")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_release_tags_pushes_created_and_floating_tags() {
	let fixture = init_release_repo();
	let root = fixture.path();
	let remote = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	git(
		remote.path(),
		&["init", "--bare", "--initial-branch", "main"],
	);
	git(
		root,
		&["remote", "add", "origin", path_to_str(remote.path())],
	);
	git(root, &["push", "-u", "origin", "main"]);

	let discovery = floating_tags_record("1.0.1", "v1.0.1", &["v{{ major }}"]);
	let discovery = resolved_discovery(root, discovery).await;

	let report = create_release_tags(root, &discovery, true, false)
		.await
		.unwrap_or_else(|error| panic!("create release tags: {error}"));

	assert_eq!(report.status, "completed");

	let remote_tags = std::process::Command::new("git")
		.args(["ls-remote", "--tags", "origin"])
		.current_dir(root)
		.output()
		.unwrap_or_else(|error| panic!("ls-remote: {error}"));
	let remote_tags = String::from_utf8_lossy(&remote_tags.stdout);
	for tag in ["refs/tags/v1.0.1", "refs/tags/v1"] {
		assert!(remote_tags.contains(tag), "expected {tag} in {remote_tags}");
	}
}

fn sample_tag_report(status: &str, operations: &[ReleaseTagOperation]) -> ReleaseTagReport {
	ReleaseTagReport {
		from: "HEAD".to_string(),
		resolved_from_commit: "def5678901234".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 1,
		push: status != "dry_run",
		dry_run: status == "dry_run",
		tag_results: operations
			.iter()
			.enumerate()
			.map(|(index, operation)| {
				ReleaseTagResult {
					tag_name: format!("v1.{index}.0"),
					target_commit: "abc1234567890".to_string(),
					existing_commit: match operation {
						ReleaseTagOperation::AlreadyUpToDate => Some("abc1234567890".to_string()),
						ReleaseTagOperation::Created if index > 0 => {
							Some("0123456789abc".to_string())
						}
						_ => None,
					},
					operation: *operation,
					floating_results: if index == 0 {
						vec![
							ReleaseFloatingTagResult {
								tag_name: "v1".to_string(),
								previous_commit: Some("0123456789abc".to_string()),
							},
							ReleaseFloatingTagResult {
								tag_name: "v1.0".to_string(),
								previous_commit: None,
							},
						]
					} else {
						Vec::new()
					},
				}
			})
			.collect(),
		tags: BTreeMap::new(),
		status: status.to_string(),
	}
}

#[test]
fn release_tag_reports_lead_with_what_happened_to_the_tags() {
	insta::assert_snapshot!(
		"release_tag_report_dry_run",
		text_release_tag_report(&sample_tag_report(
			"dry_run",
			&[ReleaseTagOperation::Planned]
		))
	);
	insta::assert_snapshot!(
		"release_tag_report_completed",
		text_release_tag_report(&sample_tag_report(
			"completed",
			&[
				ReleaseTagOperation::Created,
				ReleaseTagOperation::AlreadyUpToDate,
				ReleaseTagOperation::Created,
			]
		))
	);
	assert!(
		text_release_tag_report(&sample_tag_report(
			"already_up_to_date",
			&[ReleaseTagOperation::AlreadyUpToDate]
		))
		.starts_with("✔ Release tags are already up to date · push enabled")
	);
	let none = text_release_tag_report(&sample_tag_report("no_tags_declared", &[]));
	assert!(
		none.starts_with("• The release record declares no tags"),
		"{none}"
	);
	assert!(!none.contains("Tags ("), "{none}");
}
