#![allow(clippy::disallowed_methods)]
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetRevision;
use monochange_core::ChangesetTargetKind;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;

use super::batch_git_log;
use super::build_prepared_changesets;
use super::build_release_plan_from_signals;
use super::diagnose_changesets;
use super::discover_changeset_paths;
use super::has_semantic_guardrail_candidate;
use super::parse_batch_git_log_bytes;
use super::parse_batch_git_log_output;
use super::render_changeset_diagnostics;
use super::render_changeset_markdown;
use super::semantic_compatibility_evidence;
use super::semantic_package_targets;
use crate::ChangesetDiagnosticsReport;
use crate::PreparedChangeset;
use crate::PreparedChangesetTarget;

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn make_test_package_record(id: &str) -> PackageRecord {
	let mut package = PackageRecord::new(
		Ecosystem::Cargo,
		id.to_string(),
		PathBuf::from("crates/core/Cargo.toml"),
		PathBuf::from("crates/core"),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	package.id = id.to_string();
	package
}

fn make_test_change_signal(package_id: &str, bump: BumpSeverity) -> ChangeSignal {
	ChangeSignal {
		package_id: package_id.to_string(),
		requested_bump: Some(bump),
		explicit_version: None,
		change_origin: "test".to_string(),
		evidence_refs: Vec::new(),
		notes: None,
		details: None,
		change_type: None,
		caused_by: Vec::new(),
		source_path: PathBuf::from(".changeset/test.md"),
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnose_changesets_loads_multiple_files_with_shared_context() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	fs::create_dir_all(fixture.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::write(
		fixture.path().join(".changeset/bug-fix.md"),
		"---\ncore: patch\n---\n\nFix a bug.\n",
	)
	.unwrap_or_else(|error| panic!("write bug fix changeset: {error}"));
	fs::write(
		fixture.path().join(".changeset/feature.md"),
		"---\ncore: minor\n---\n\nAdd a feature.\n",
	)
	.unwrap_or_else(|error| panic!("write feature changeset: {error}"));

	let report = diagnose_changesets(fixture.path(), &[])
		.await
		.unwrap_or_else(|error| panic!("diagnose changesets: {error}"));

	assert_eq!(
		report.requested_changesets,
		vec![
			PathBuf::from(".changeset/bug-fix.md"),
			PathBuf::from(".changeset/feature.md")
		]
	);
	assert_eq!(report.changesets.len(), 2);
	assert!(report.changesets.iter().all(|changeset| {
		changeset
			.targets
			.iter()
			.any(|target| target.id == "core" && target.kind == ChangesetTargetKind::Package)
	}));
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnose_changesets_uses_configuration_index_before_workspace_discovery() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(tempdir.path().join("crates/core/Cargo.toml"), "not toml\n")
		.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(
		tempdir.path().join(".changeset/core.md"),
		"---\ncore: patch\n---\n\nFix core.\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\n\
		package_type = \"cargo\"\n\
		\n\
		[package.core]\n\
		path = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let report = diagnose_changesets(tempdir.path(), &[])
		.await
		.unwrap_or_else(|error| panic!("diagnose changesets: {error}"));

	assert_eq!(report.changesets.len(), 1);
	assert!(
		report.changesets[0]
			.targets
			.iter()
			.any(|target| { target.id == "core" && target.kind == ChangesetTargetKind::Package })
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnose_changesets_falls_back_to_workspace_versions_for_explicit_versions() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"real-core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(
		tempdir.path().join(".changeset/core.md"),
		"---\ncore:\n  version: 1.2.0\n---\n\nPin core.\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\n\
		package_type = \"cargo\"\n\
		\n\
		[package.core]\n\
		path = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let report = diagnose_changesets(tempdir.path(), &[])
		.await
		.unwrap_or_else(|error| panic!("diagnose changesets: {error}"));
	let target = report.changesets[0]
		.targets
		.iter()
		.find(|target| target.id == "core")
		.unwrap_or_else(|| panic!("expected core target"));
	assert_eq!(target.bump, Some(BumpSeverity::Minor));
}

#[test]
fn render_changeset_markdown_streams_targets_and_details() {
	let fixture = setup_fixture("changeset-target-metadata/render-workspace");
	let configuration = crate::load_workspace_configuration(fixture.path())
		.unwrap_or_else(|error| panic!("load configuration: {error}"));
	let package_refs = vec!["core".to_string(), "sdk".to_string()];
	let caused_by = vec!["sdk".to_string(), "pkg\"name".to_string()];

	let markdown = render_changeset_markdown(
		&configuration,
		&package_refs,
		BumpSeverity::Patch,
		Some("2.0.0"),
		"Ship runtime",
		Some("security"),
		&caused_by,
		Some("\nMore details\n"),
	)
	.unwrap_or_else(|error| panic!("render changeset markdown: {error}"));

	assert_eq!(
		markdown,
		concat!(
			"---\n",
			"core:\n",
			"  bump: patch\n",
			"  type: security\n",
			"  version: \"2.0.0\"\n",
			"  caused_by: [\"sdk\", \"pkg\\\"name\"]\n",
			"sdk:\n",
			"  bump: patch\n",
			"  type: security\n",
			"  version: \"2.0.0\"\n",
			"  caused_by: [\"sdk\", \"pkg\\\"name\"]\n",
			"---\n",
			"\n",
			"# Ship runtime\n",
			"\n",
			"More details\n",
		)
	);
}

#[test]
fn render_changeset_diagnostics_streams_text_without_temporary_lines() {
	let report = ChangesetDiagnosticsReport {
		requested_changesets: vec![
			PathBuf::from(".changeset/feature.md"),
			PathBuf::from(".changeset/minimal.md"),
		],
		changesets: vec![
			PreparedChangeset {
				path: PathBuf::from(".changeset/feature.md"),
				summary: Some("ship feature".to_string()),
				details: Some("long details".to_string()),
				targets: vec![
					PreparedChangesetTarget {
						id: "core".to_string(),
						kind: ChangesetTargetKind::Package,
						bump: Some(BumpSeverity::Minor),
						origin: "manual".to_string(),
						evidence_refs: vec!["src/lib.rs".to_string(), "README.md".to_string()],
						change_type: Some("feature".to_string()),
						caused_by: vec!["core".to_string(), "api".to_string()],
					},
					PreparedChangesetTarget {
						id: "web".to_string(),
						kind: ChangesetTargetKind::Package,
						bump: None,
						origin: "inferred".to_string(),
						evidence_refs: Vec::new(),
						change_type: None,
						caused_by: Vec::new(),
					},
				],
				context: Some(ChangesetContext {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					capabilities: HostingCapabilities::default(),
					introduced: Some(ChangesetRevision {
						actor: None,
						commit: Some(HostedCommitRef {
							provider: HostingProviderKind::GitHub,
							host: Some("github.com".to_string()),
							sha: "abc123456789".to_string(),
							short_sha: "abc1234".to_string(),
							url: Some(
								"https://github.com/example/repo/commit/abc123456789".to_string(),
							),
							authored_at: None,
							committed_at: None,
							author_name: None,
							author_email: None,
						}),
						review_request: Some(HostedReviewRequestRef {
							provider: HostingProviderKind::GitHub,
							host: Some("github.com".to_string()),
							kind: HostedReviewRequestKind::PullRequest,
							id: "#42".to_string(),
							title: None,
							url: Some("https://github.com/example/repo/pull/42".to_string()),
							author: None,
						}),
					}),
					last_updated: Some(ChangesetRevision {
						actor: None,
						commit: Some(HostedCommitRef {
							provider: HostingProviderKind::GitHub,
							host: Some("github.com".to_string()),
							sha: "def123456789".to_string(),
							short_sha: "def1234".to_string(),
							url: None,
							authored_at: None,
							committed_at: None,
							author_name: None,
							author_email: None,
						}),
						review_request: None,
					}),
					related_issues: vec![
						HostedIssueRef {
							provider: HostingProviderKind::GitHub,
							host: Some("github.com".to_string()),
							id: "#99".to_string(),
							title: None,
							url: None,
							relationship: HostedIssueRelationshipKind::Mentioned,
						},
						HostedIssueRef {
							provider: HostingProviderKind::GitHub,
							host: Some("github.com".to_string()),
							id: "#100".to_string(),
							title: None,
							url: None,
							relationship: HostedIssueRelationshipKind::Mentioned,
						},
					],
				}),
			},
			PreparedChangeset {
				path: PathBuf::from(".changeset/minimal.md"),
				summary: None,
				details: None,
				targets: Vec::new(),
				context: Some(ChangesetContext {
					provider: HostingProviderKind::GenericGit,
					host: None,
					capabilities: HostingCapabilities::default(),
					introduced: None,
					last_updated: Some(ChangesetRevision {
						actor: None,
						commit: None,
						review_request: Some(HostedReviewRequestRef {
							provider: HostingProviderKind::GitHub,
							host: Some("github.com".to_string()),
							kind: HostedReviewRequestKind::PullRequest,
							id: "#77".to_string(),
							title: None,
							url: None,
							author: None,
						}),
					}),
					related_issues: Vec::new(),
				}),
			},
		],
	};

	let rendered = render_changeset_diagnostics(&report);

	assert_eq!(
		rendered,
		concat!(
			"changeset: .changeset/feature.md\n",
			"  summary: ship feature\n",
			"  details: long details\n",
			"  targets:\n",
			"  - package core (bump: minor, origin: manual)\n",
			"    caused by: core, api\n",
			"    evidence: src/lib.rs, README.md\n",
			"  - package web (bump: auto, origin: inferred)\n",
			"  introduced: abc1234\n",
			"  last-updated: def1234\n",
			"  review request: #42 (https://github.com/example/repo/pull/42)\n",
			"  related issues: #99, #100\n",
			"\n",
			"changeset: .changeset/minimal.md\n",
			"  summary: <missing summary>\n",
			"  review request: #77",
		)
	);
	assert_eq!(
		render_changeset_diagnostics(&ChangesetDiagnosticsReport {
			requested_changesets: Vec::new(),
			changesets: Vec::new(),
		}),
		"no matching changesets found"
	);
}

#[test]
fn build_prepared_changesets_moves_loaded_fields_into_prepared_changesets() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::write(
		tempdir.path().join(".changeset/feature.md"),
		"---\ncore: patch\n---\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	let loaded = vec![monochange_config::LoadedChangesetFile {
		path: tempdir.path().join(".changeset/feature.md"),
		summary: Some("feature".to_string()),
		details: Some("details".to_string()),
		targets: vec![monochange_config::LoadedChangesetTarget {
			id: "core".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: Some(BumpSeverity::Patch),
			explicit_version: None,
			origin: "manual".to_string(),
			evidence_refs: vec!["src/lib.rs".to_string()],
			change_type: Some("fix".to_string()),
			caused_by: vec!["core".to_string()],
		}],
		signals: Vec::new(),
	}];

	let prepared = build_prepared_changesets(tempdir.path(), loaded);

	assert_eq!(prepared.len(), 1);
	assert_eq!(prepared[0].path, PathBuf::from(".changeset/feature.md"));
	assert_eq!(prepared[0].summary.as_deref(), Some("feature"));
	assert_eq!(prepared[0].details.as_deref(), Some("details"));
	assert_eq!(prepared[0].targets.len(), 1);
	assert_eq!(prepared[0].targets[0].id, "core");
	assert_eq!(prepared[0].targets[0].origin, "manual");
	assert_eq!(prepared[0].targets[0].evidence_refs, vec!["src/lib.rs"]);
	assert_eq!(prepared[0].targets[0].change_type.as_deref(), Some("fix"));
	assert_eq!(prepared[0].targets[0].caused_by, vec!["core"]);
	let context = prepared[0]
		.context
		.as_ref()
		.unwrap_or_else(|| panic!("expected generic git context"));
	assert_eq!(context.provider, HostingProviderKind::GenericGit);
}

#[test]
fn discover_changeset_paths_reports_io_for_non_directory_changeset_path() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join(".changeset"), "not a directory")
		.unwrap_or_else(|error| panic!("write changeset marker: {error}"));

	let error = discover_changeset_paths(tempdir.path(), false)
		.err()
		.unwrap_or_else(|| panic!("expected read_dir failure"));
	let message = error.to_string();

	assert!(
		message.contains("failed to read"),
		"expected read failure, got {message}"
	);
	assert!(
		message.contains(".changeset"),
		"expected changeset path, got {message}"
	);
}

#[test]
fn batch_git_log_returns_empty_maps_for_empty_paths() {
	let (introduced, last_updated) = batch_git_log(Path::new("."), &[]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn batch_git_log_returns_empty_maps_when_git_log_fails() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let (introduced, last_updated) =
		batch_git_log(tempdir.path(), &[PathBuf::from(".changeset/feature.md")]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn parse_batch_git_log_bytes_returns_empty_maps_for_invalid_utf8_output() {
	let (introduced, last_updated) =
		parse_batch_git_log_bytes(b"\xff", &[PathBuf::from(".changeset/feature.md")]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn parse_batch_git_log_output_ignores_malformed_name_status_lines() {
	let (introduced, last_updated) = parse_batch_git_log_output(
		"abc123\x1fIfiok\x1fifiok@example.com\x1f2026-04-06T00:00:00Z\x1f2026-04-06T00:00:00Z\nM\n",
		&[PathBuf::from(".changeset/feature.md")],
	);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn semantic_guardrail_skips_non_git_roots_without_warning() {
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let signal = make_test_change_signal("core", BumpSeverity::Patch);

	let (evidence, warnings) = semantic_compatibility_evidence(tempdir.path(), &[], &[signal]);

	assert!(evidence.is_empty());
	assert!(warnings.is_empty());
}

#[test]
fn semantic_guardrail_skips_changeset_only_updates_without_warning() {
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	run_git_test_command(tempdir.path(), &["init", "-b", "main"]);
	run_git_test_command(tempdir.path(), &["config", "user.name", "monochange test"]);
	run_git_test_command(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::write(tempdir.path().join(".changeset/core.md"), "initial")
		.unwrap_or_else(|error| panic!("write initial changeset: {error}"));
	run_git_test_command(tempdir.path(), &["add", "."]);
	run_git_test_command(tempdir.path(), &["commit", "-m", "initial fixture"]);
	fs::write(tempdir.path().join(".changeset/core.md"), "updated")
		.unwrap_or_else(|error| panic!("write updated changeset: {error}"));
	let signal = make_test_change_signal("core", BumpSeverity::Patch);

	let (evidence, warnings) = semantic_compatibility_evidence(tempdir.path(), &[], &[signal]);

	assert!(evidence.is_empty());
	assert!(warnings.is_empty());
}

#[test]
fn semantic_guardrail_candidate_detection_uses_supported_source_extensions() {
	assert!(has_semantic_guardrail_candidate(&[PathBuf::from(
		"crates/core/src/lib.rs"
	)]));
	assert!(has_semantic_guardrail_candidate(&[PathBuf::from(
		"packages/core/package.json"
	)]));
	assert!(!has_semantic_guardrail_candidate(&[PathBuf::from(
		".changeset/core.md"
	)]));
	assert!(!has_semantic_guardrail_candidate(&[PathBuf::from(
		"README"
	)]));
}

#[test]
fn semantic_package_targets_skips_packages_without_changesets() {
	let packages = vec![
		make_test_package_record("core"),
		make_test_package_record("other"),
	];
	let changeset_packages = BTreeSet::from(["core"]);

	let targets = semantic_package_targets(&packages, &changeset_packages);

	assert_eq!(targets.get("core").map(String::as_str), Some("core"));
	assert!(!targets.contains_key("other"));
}

#[test]
fn release_plan_propagates_graph_errors_after_semantic_guardrail_collection() {
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let mut configuration = monochange_config::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("load configuration: {error}"));
	configuration.root_path = tempdir.path().to_path_buf();
	let discovery = DiscoveryReport {
		workspace_root: tempdir.path().to_path_buf(),
		packages: vec![make_test_package_record("core")],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let mut signal = make_test_change_signal("core", BumpSeverity::Patch);
	signal.explicit_version = Some(Version::new(1, 0, 0));

	let error = build_release_plan_from_signals(&configuration, &discovery, &[signal])
		.expect_err("unknown signal target should fail release planning");

	assert!(error.to_string().contains("greater than current version"));
}

fn run_git_test_command(root: &Path, args: &[&str]) {
	let mut command = std::process::Command::new("git");
	command.arg("-C").arg(root);
	if args.first() == Some(&"commit") {
		command.arg("-c").arg("commit.gpgsign=false");
	}
	let status = command
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn replace_workspace_contents(destination: &Path, source: &Path) {
	for entry in
		fs::read_dir(destination).unwrap_or_else(|error| panic!("read temp workspace: {error}"))
	{
		let entry = entry.unwrap_or_else(|error| panic!("read temp entry: {error}"));
		if entry.file_name() == ".git" {
			continue;
		}
		let path = entry.path();
		if path.is_dir() {
			fs::remove_dir_all(&path)
				.unwrap_or_else(|error| panic!("remove dir {}: {error}", path.display()));
		} else {
			fs::remove_file(&path)
				.unwrap_or_else(|error| panic!("remove file {}: {error}", path.display()));
		}
	}
	monochange_test_helpers::fs::copy_directory(source, destination);
}

fn setup_semantic_guardrail_git_fixture(relative: &str) -> tempfile::TempDir {
	let before = monochange_test_helpers::fs::fixture_path_from(
		env!("CARGO_MANIFEST_DIR"),
		&format!("semantic-release-guardrails/{relative}/before"),
	);
	let after = monochange_test_helpers::fs::fixture_path_from(
		env!("CARGO_MANIFEST_DIR"),
		&format!("semantic-release-guardrails/{relative}/after"),
	);
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	monochange_test_helpers::fs::copy_directory(&before, tempdir.path());
	run_git_test_command(tempdir.path(), &["init", "-b", "main"]);
	run_git_test_command(tempdir.path(), &["config", "user.name", "monochange test"]);
	run_git_test_command(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	run_git_test_command(tempdir.path(), &["add", "."]);
	run_git_test_command(tempdir.path(), &["commit", "-m", "initial fixture"]);
	replace_workspace_contents(tempdir.path(), &after);
	tempdir
}

#[test]
fn semantic_guardrail_warns_when_change_frame_detection_fails_in_git_root() {
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join(".git"), "not a git directory")
		.unwrap_or_else(|error| panic!("write git marker: {error}"));
	let signal = make_test_change_signal("core", BumpSeverity::Patch);

	let (evidence, warnings) = semantic_compatibility_evidence(tempdir.path(), &[], &[signal]);

	assert!(evidence.is_empty());
	assert!(
		warnings
			.iter()
			.any(|warning| warning.contains("change frame could not be detected"))
	);
}

#[test]
fn semantic_guardrail_warns_when_semantic_analysis_fails() {
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	run_git_test_command(tempdir.path(), &["init", "-b", "main"]);
	run_git_test_command(tempdir.path(), &["config", "user.name", "monochange test"]);
	run_git_test_command(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	fs::write(tempdir.path().join("monochange.toml"), "packages = [")
		.unwrap_or_else(|error| panic!("write invalid config: {error}"));
	run_git_test_command(tempdir.path(), &["add", "."]);
	run_git_test_command(tempdir.path(), &["commit", "-m", "initial fixture"]);
	fs::write(
		tempdir.path().join("monochange.toml"),
		"packages = [invalid",
	)
	.unwrap_or_else(|error| panic!("write invalid changed config: {error}"));
	let signal = make_test_change_signal("core", BumpSeverity::Patch);

	let (evidence, warnings) = semantic_compatibility_evidence(tempdir.path(), &[], &[signal]);

	assert!(evidence.is_empty());
	assert!(
		warnings
			.iter()
			.any(|warning| warning.contains("semantic analysis failed"))
	);
}

#[test]
fn semantic_guardrail_warns_for_semantic_changes_without_matching_changeset_target() {
	let fixture = setup_semantic_guardrail_git_fixture("changeset-covered-breaking-public-api");
	let signal = make_test_change_signal("other", BumpSeverity::Patch);

	let (evidence, warnings) = semantic_compatibility_evidence(fixture.path(), &[], &[signal]);

	assert!(evidence.is_empty());
	assert!(
		warnings
			.iter()
			.any(|warning| warning.contains("no pending changeset targets"))
	);
}
