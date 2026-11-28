#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangelogSettings;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleasePlan;
use monochange_core::ShellConfig;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use serde::Serialize;
use tempfile::TempDir;
use tempfile::tempdir;

use super::*;
use crate::tests::TEST_ENV_LOCK;

async fn execute_matches(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command_name: &str,
	cli_command_matches: &ArgMatches,
	quiet: bool,
) -> MonochangeResult<String> {
	execute_matches_with_progress(
		root,
		configuration,
		cli_command_name,
		cli_command_matches,
		quiet,
		None,
	)
	.await
}

fn cli_context() -> CliContext {
	CliContext {
		root: PathBuf::from("."),
		dry_run: false,
		quiet: false,
		show_diff: false,
		output_format: OutputFormat::Text,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		originating_command: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
		command_results: Vec::new(),
	}
}

#[test]
fn resolve_step_input_template_uses_fast_paths_for_literals_and_inputs() {
	let mut context = cli_context();
	context
		.inputs
		.insert("no_verify".to_string(), vec!["true".to_string()]);
	let mut template_context = None;

	let literal = resolve_step_input_template("HEAD", &context, &mut template_context)
		.unwrap_or_else(|error| panic!("resolve literal input: {error}"));
	assert_eq!(literal, vec!["HEAD".to_string()]);
	assert!(template_context.is_none());

	let inherited =
		resolve_step_input_template("{{ inputs.no_verify }}", &context, &mut template_context)
			.unwrap_or_else(|error| panic!("resolve direct input template: {error}"));
	assert_eq!(inherited, vec!["true".to_string()]);
	assert!(template_context.is_none());

	let nested_input_path = resolve_step_input_template(
		"{{ inputs.no_verify.enabled }}",
		&context,
		&mut template_context,
	)
	.unwrap_or_else(|error| panic!("resolve nested input path template: {error}"));
	assert!(nested_input_path.is_empty());
	assert!(template_context.is_some());
}

#[test]
fn evaluate_fast_cli_step_condition_handles_changeset_count_and_inputs() {
	let mut context = cli_context();
	let mut prepared_release = sample_prepared_release();
	prepared_release
		.changeset_paths
		.push(PathBuf::from(".changeset/release.md"));
	context.prepared_release = Some(prepared_release);
	context
		.inputs
		.insert("commit".to_string(), vec!["true".to_string()]);
	let step_inputs = BTreeMap::from([("push".to_string(), vec!["false".to_string()])]);
	context.inputs.insert("tag".to_string(), Vec::new());

	assert_eq!(
		evaluate_fast_cli_step_condition(
			"{{ number_of_changesets > 0 && inputs.commit }}",
			&context,
			&step_inputs,
		),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition("{{ changeset_count <= 1 }}", &context, &step_inputs),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition("{{ changeset_count == 1 }}", &context, &step_inputs),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition("{{ changeset_count != 0 }}", &context, &step_inputs),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition("{{ changeset_count < 2 }}", &context, &step_inputs),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition("{{ !inputs.push }}", &context, &step_inputs),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition("{{ not inputs.push }}", &context, &step_inputs),
		Some(true)
	);
	assert_eq!(
		evaluate_fast_cli_step_condition(
			"{{ number_of_changesets > 0 && inputs.commit && inputs.push }}",
			&context,
			&step_inputs,
		),
		Some(false)
	);

	assert_eq!(
		evaluate_fast_cli_step_condition(
			"{{ number_of_changesets > 0 && inputs.tag }}",
			&context,
			&step_inputs,
		),
		Some(false)
	);
	context
		.prepared_release
		.as_mut()
		.unwrap()
		.changeset_paths
		.clear();
	assert_eq!(
		evaluate_fast_cli_step_condition(
			"{{ number_of_changesets > 0 && inputs.missing }}",
			&context,
			&BTreeMap::new(),
		),
		Some(false)
	);
}

fn sample_source_configuration() -> SourceConfiguration {
	let provider = serde_json::from_str::<SourceProvider>("\"github\"")
		.unwrap_or_else(|error| panic!("source provider: {error}"));
	SourceConfiguration {
		provider,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn build_release_request_result_for_source_real_mode_delegates_to_publisher() {
	let source = sample_source_configuration();
	let request = monochange_core::SourceChangeRequest {
		provider: source.provider,
		repository: "monochange/monochange".to_string(),
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		base_branch: "main".to_string(),
		head_branch: "release/monochange".to_string(),
		title: "Release monochange".to_string(),
		body: "Release notes".to_string(),
		labels: Vec::new(),
		auto_merge: false,
		commit_message: monochange_core::CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		body_truncation: None,
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	let error = build_release_request_result_for_source(
		false,
		&source,
		tempdir.path(),
		&request,
		&[],
		false,
		true,
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected publish error"));

	assert!(
		error
			.to_string()
			.contains("failed to prepare release pull request branch"),
		"unexpected error: {error}"
	);
}

fn sample_configuration(root: &Path) -> monochange_core::WorkspaceConfiguration {
	monochange_core::WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
		swift: monochange_core::EcosystemSettings::default(),
		cocoapods: monochange_core::EcosystemSettings::default(),
		gradle: monochange_core::EcosystemSettings::default(),
		version_schemes: BTreeMap::new(),
	}
}

fn sample_prepared_release() -> PreparedRelease {
	PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: Vec::new(),
		package_publications: Vec::new(),
		version: None,
		group_version: None,
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
		versioning: crate::versioning_state::ResolvedReleaseValues::default(),
	}
}

fn sample_prepared_release_with_versions() -> PreparedRelease {
	PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![
				monochange_core::ReleaseDecision {
					package_id: "core".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Minor,
					planned_version: Some(semver::Version::new(1, 2, 0)),
					group_id: Some("sdk".to_string()),
					reasons: vec!["feature".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				monochange_core::ReleaseDecision {
					package_id: "web".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Patch,
					planned_version: Some(semver::Version::new(1, 2, 1)),
					group_id: Some("sdk".to_string()),
					reasons: vec!["fix".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				monochange_core::ReleaseDecision {
					package_id: "docs".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::None,
					planned_version: Some(semver::Version::new(9, 9, 9)),
					group_id: None,
					reasons: Vec::new(),
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
			],
			groups: vec![monochange_core::PlannedVersionGroup {
				group_id: "sdk".to_string(),
				display_name: "SDK".to_string(),
				members: vec!["core".to_string(), "web".to_string()],
				mismatch_detected: false,
				planned_version: Some(semver::Version::new(2, 0, 0)),
				recommended_bump: BumpSeverity::Minor,
			}],
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string(), "web".to_string()],
		package_publications: Vec::new(),
		version: Some("1.2.1".to_string()),
		group_version: Some("2.0.0".to_string()),
		release_targets: Vec::new(),
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
		versioning: crate::versioning_state::ResolvedReleaseValues::default(),
	}
}

fn parse_validate_matches(root: &Path) -> (monochange_core::WorkspaceConfiguration, ArgMatches) {
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
	let matches = build_command_with_cli("monochange", &configuration.cli)
		.try_get_matches_from(["monochange", "step", "discover"])
		.unwrap_or_else(|error| panic!("discover matches: {error}"));
	(configuration, matches)
}

fn default_cli_command(name: &str) -> CliCommandDefinition {
	synthetic_step_command_definition(name)
		.unwrap_or_else(|error| panic!("expected default cli command `{name}`: {error}"))
}

fn release_cli_command() -> CliCommandDefinition {
	let mut command = default_cli_command("prepare-release");
	command.name = "release".to_string();
	command
}

fn format_input(format: &str) -> BTreeMap<String, Vec<String>> {
	BTreeMap::from([("format".to_string(), vec![format.to_string()])])
}

fn sample_package_publish_outcome(
	status: package_publish::PackagePublishStatus,
	trust_status: package_publish::TrustedPublishingStatus,
) -> package_publish::PackagePublishOutcome {
	package_publish::PackagePublishOutcome {
		package: "@scope/pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		registry: "npm".to_string(),
		version: "1.2.3".to_string(),
		status,
		message: "published package to npm".to_string(),
		placeholder: false,
		trusted_publishing: package_publish::TrustedPublishingOutcome {
			status: trust_status,
			repository: Some("monochange/monochange".to_string()),
			workflow: Some("publish.yml".to_string()),
			environment: Some("release".to_string()),
			setup_url: Some("https://docs.npmjs.com/cli/v11/commands/npm-trust".to_string()),
			message: "trusted publishing already configured".to_string(),
		},
		command: None,
		stdout: None,
		stderr: None,
		rate_limit: None,
		deferred_to: None,
	}
}

fn sample_rate_limit_report() -> monochange_core::PublishRateLimitReport {
	monochange_core::PublishRateLimitReport {
		dry_run: true,
		windows: vec![monochange_core::RegistryRateLimitWindowPlan {
			registry: monochange_core::RegistryKind::PubDev,
			operation: monochange_core::RateLimitOperation::Publish,
			limit: Some(12),
			window_seconds: Some(86_400),
			pending: 13,
			batches_required: 2,
			fits_single_window: false,
			confidence: monochange_core::RateLimitConfidence::Medium,
			notes: "pub.dev limit".to_string(),
			evidence: Vec::new(),
		}],
		batches: vec![
			monochange_core::PublishRateLimitBatch {
				registry: monochange_core::RegistryKind::PubDev,
				operation: monochange_core::RateLimitOperation::Publish,
				batch_index: 1,
				total_batches: 2,
				packages: vec!["pkg-a".to_string()],
				recommended_wait_seconds: None,
			},
			monochange_core::PublishRateLimitBatch {
				registry: monochange_core::RegistryKind::PubDev,
				operation: monochange_core::RateLimitOperation::Publish,
				batch_index: 2,
				total_batches: 2,
				packages: vec!["pkg-b".to_string()],
				recommended_wait_seconds: Some(86_400),
			},
		],
		warnings: vec!["needs 2 batches".to_string()],
	}
}

fn git_in_dir(root: &Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn prepared_release_git_workspace() -> TempDir {
	let workspace_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = workspace_dir.path();
	fs::write(root.join("Cargo.toml"), "[workspace]\n")
		.unwrap_or_else(|error| panic!("write workspace manifest: {error}"));
	git_in_dir(root, &["init", "-b", "main"]);
	git_in_dir(root, &["config", "user.name", "monochange Tests"]);
	git_in_dir(root, &["config", "user.email", "monochange@example.com"]);
	git_in_dir(root, &["config", "commit.gpgsign", "false"]);
	git_in_dir(root, &["add", "Cargo.toml"]);

	git_in_dir(root, &["commit", "-m", "initial"]);
	workspace_dir
}

#[test]
fn render_cli_command_results_include_release_details_policy_and_logs() {
	let cli_command = default_cli_command("prepare-release");
	let mut context = cli_context();
	context.show_diff = true;
	context.release_manifest_path = Some(PathBuf::from(".monochange/local/release.json"));
	context.release_results = vec!["published v1.2.3".to_string()];
	context.release_request_result = Some("opened release request".to_string());
	context.issue_comment_results = vec!["commented on #42".to_string()];
	context.release_commit_report = Some(CommitReleaseReport {
		subject: "chore(release): publish".to_string(),
		body: "body".to_string(),
		commit: Some("abcdef1234567890".to_string()),
		tracked_paths: vec![PathBuf::from("Cargo.toml")],
		dry_run: false,
		verified: Some(true),
		status: "completed".to_string(),
	});

	context.prepared_release = Some(PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: vec![PathBuf::from(".changeset/feature.md")],
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: None,
		release_targets: vec![ReleaseTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: Vec::new(),
			rendered_title: "core v1.2.3".to_string(),
			rendered_changelog_title: "core v1.2.3".to_string(),
			floating_tags: Vec::new(),
		}],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: vec![PathBuf::from(".changeset/feature.md")],
		package_publications: Vec::new(),
		dry_run: true,
		versioning: crate::versioning_state::ResolvedReleaseValues::default(),
	});
	context.prepared_file_diffs = vec![PreparedFileDiff {
		path: PathBuf::from("Cargo.toml"),
		diff: "-old\n+new".to_string(),
		display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
	}];
	context.command_logs = vec!["ran cargo check".to_string()];
	context.changeset_policy_evaluation = Some(ChangesetPolicyEvaluation {
		enforce: true,
		required: true,
		status: ChangesetPolicyStatus::Failed,
		summary: "coverage missing".to_string(),
		comment: None,
		labels: Vec::new(),
		matched_skip_labels: vec!["skip-changeset".to_string()],
		changed_paths: vec!["crates/core/src/lib.rs".to_string()],
		matched_paths: vec!["crates/core/src/lib.rs".to_string()],
		ignored_paths: Vec::new(),
		changeset_paths: vec![".changeset/feature.md".to_string()],
		affected_package_ids: vec!["core".to_string()],
		covered_package_ids: Vec::new(),
		uncovered_package_ids: vec!["core".to_string()],
		warnings: Vec::new(),
		errors: vec!["missing changeset".to_string()],
	});

	let text = render_cli_command_result(&cli_command, &context);
	insta::assert_snapshot!("release_details_policy_and_logs_text", text);

	let markdown = render_cli_command_markdown_result(&cli_command, &context);
	assert!(markdown.contains("## Release targets"));
	assert!(markdown.contains("## Release manifest"));
	assert!(markdown.contains("## Release commit"));
	assert!(markdown.contains("## Changed files"));
	assert!(markdown.contains("## File diffs"));
	assert!(markdown.contains("## Deleted changesets"));
	assert!(markdown.contains("## Commands"));
}

#[test]
fn release_version_summary_renderers_cover_empty_and_single_section_states() {
	let empty_release = sample_prepared_release();
	let empty = build_release_version_summary(&empty_release);
	let no_labels = BTreeMap::new();
	assert_eq!(
		render_release_version_summary_text(&empty, &no_labels),
		"• No versions planned · add a changeset with `monochange create` to plan a release"
	);
	assert_eq!(
		render_release_version_summary_markdown(&empty),
		"No package or group versions were planned."
	);

	let mut groups_release = sample_prepared_release_with_versions();
	groups_release.plan.decisions.clear();
	let groups_only = build_release_version_summary(&groups_release);
	assert_eq!(
		render_release_version_summary_text(&groups_only, &no_labels),
		"Groups\n  sdk  2.0.0  minor\n\n✔ Next versions · 1 group · 0 packages"
	);
	assert_eq!(
		render_release_version_summary_markdown(&groups_only),
		"## Group versions\n\n- `sdk`: `2.0.0`"
	);

	let mut packages_release = sample_prepared_release_with_versions();
	packages_release.plan.groups.clear();
	let packages_only = build_release_version_summary(&packages_release);
	let labels = BTreeMap::from([("core".to_string(), "core-lib".to_string())]);
	assert_eq!(
		render_release_version_summary_text(&packages_only, &labels),
		"Packages\n  core-lib  1.2.0  minor\n  web       1.2.1  patch\n\n✔ Next versions · 2 packages"
	);
	assert_eq!(
		render_release_version_summary_markdown(&packages_only),
		"## Package versions\n\n- `core`: `1.2.0`\n- `web`: `1.2.1`"
	);
}

#[test]
fn publish_rendering_ends_with_the_outcome_summary() {
	let mut context = cli_context();
	context.package_publish_report = Some(package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: false,
		packages: vec![
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Published,
				package_publish::TrustedPublishingStatus::Configured,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::SkippedExisting,
				package_publish::TrustedPublishingStatus::Configured,
			),
		],
	});
	let command = CliCommandDefinition {
		name: "placeholder".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};

	let text = render_cli_command_result(&command, &context);
	assert!(text.starts_with("Published (1)\n  @scope/pkg"));
	assert!(text.ends_with("\n\n✔ Published 1 placeholder package · 1 already published"));
	assert_eq!(text.matches("@scope/pkg").count(), 1);
	assert!(!text.contains("command `placeholder` completed"));

	let markdown = render_cli_command_markdown_result(&command, &context);
	let publishing = markdown
		.find("## Placeholder publishing")
		.expect("publish section");
	assert!(markdown[publishing..].contains("**Published 1 placeholder package**"));
}

fn render_publish_text(report: &package_publish::PackagePublishReport, show_all: bool) -> String {
	let mut text = TextReport::new(TextTheme::for_stdout());
	render_package_publish_report(&mut text, report, show_all);
	text.render()
}

#[test]
fn package_publish_outcome_lines_distinguish_every_outcome() {
	let report = |mode, dry_run, statuses: &[package_publish::PackagePublishStatus]| {
		package_publish::PackagePublishReport {
			mode,
			dry_run,
			packages: statuses
				.iter()
				.map(|status| {
					sample_package_publish_outcome(
						*status,
						package_publish::TrustedPublishingStatus::Disabled,
					)
				})
				.collect(),
		}
	};
	let headline = |report: &package_publish::PackagePublishReport| {
		package_publish_outcome_line(report, report.summary()).1
	};

	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Placeholder,
			false,
			&[],
		)),
		"No packages matched placeholder publishing criteria"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Placeholder,
			false,
			&[package_publish::PackagePublishStatus::SkippedExisting],
		)),
		"No placeholder packages need publishing"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[package_publish::PackagePublishStatus::SkippedExisting],
		)),
		"No packages need publishing"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Placeholder,
			true,
			&[package_publish::PackagePublishStatus::Planned],
		)),
		"Would publish 1 placeholder package"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[package_publish::PackagePublishStatus::Blocked],
		)),
		"Publishing blocked for 1 package"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[package_publish::PackagePublishStatus::Failed],
		)),
		"Publishing failed for 1 package"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[
				package_publish::PackagePublishStatus::Published,
				package_publish::PackagePublishStatus::Failed,
			],
		)),
		"Published 1 package; 1 failed"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[package_publish::PackagePublishStatus::Published],
		)),
		"Published 1 package"
	);

	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[package_publish::PackagePublishStatus::Staged],
		)),
		"Staged 1 package awaiting approval"
	);
	assert_eq!(
		headline(&report(
			package_publish::PackagePublishRunMode::Release,
			false,
			&[
				package_publish::PackagePublishStatus::Published,
				package_publish::PackagePublishStatus::Staged,
			],
		)),
		"Published 1 package; staged 1 package awaiting approval"
	);
}

#[test]
fn package_publish_counts_explain_existing_and_failed_packages() {
	let report = |mode, statuses: &[package_publish::PackagePublishStatus]| {
		package_publish::PackagePublishReport {
			mode,
			dry_run: false,
			packages: statuses
				.iter()
				.map(|status| {
					sample_package_publish_outcome(
						*status,
						package_publish::TrustedPublishingStatus::Disabled,
					)
				})
				.collect(),
		}
	};
	let counts = |report: &package_publish::PackagePublishReport| {
		package_publish_counts(report, report.summary())
	};

	assert_eq!(
		counts(&report(
			package_publish::PackagePublishRunMode::Placeholder,
			&[package_publish::PackagePublishStatus::SkippedExisting],
		)),
		"Checked 1 package. All placeholder versions already exist."
	);
	assert_eq!(
		counts(&report(
			package_publish::PackagePublishRunMode::Release,
			&[package_publish::PackagePublishStatus::SkippedExisting],
		)),
		"Checked 1 package. All selected versions already exist."
	);
	assert_eq!(
		counts(&report(
			package_publish::PackagePublishRunMode::Release,
			&[
				package_publish::PackagePublishStatus::SkippedExisting,
				package_publish::PackagePublishStatus::SkippedExisting,
				package_publish::PackagePublishStatus::Failed,
			],
		)),
		"Checked 3 packages. 2 package versions already exist. 1 failed."
	);
	assert_eq!(
		counts(&report(
			package_publish::PackagePublishRunMode::Release,
			&[package_publish::PackagePublishStatus::Staged],
		)),
		"Checked 1 package. 1 staged awaiting approval (`npm stage list`, then `npm stage approve <stage-id>`). 0 failed."
	);
}

#[test]
fn render_package_publish_reports_cover_empty_and_detailed_variants() {
	let empty_placeholder = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: true,
		packages: Vec::new(),
	};
	assert_eq!(
		render_publish_text(&empty_placeholder, false),
		"• No packages matched placeholder publishing criteria"
	);
	assert_eq!(
		render_package_publish_report_markdown(&empty_placeholder, false),
		vec!["**No packages matched placeholder publishing criteria**".to_string()]
	);

	let detailed_report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_package_publish_outcome(
			package_publish::PackagePublishStatus::SkippedExternal,
			package_publish::TrustedPublishingStatus::ManualActionRequired,
		)],
	};
	let text = render_publish_text(&detailed_report, true);
	assert!(text.contains("repository: monochange/monochange"));
	assert!(text.contains("workflow: publish.yml"));
	assert!(text.contains("environment: release"));
	assert!(text.contains("setup: https://docs.npmjs.com/cli/v11/commands/npm-trust"));

	let markdown = render_package_publish_report_markdown(&detailed_report, true).join("\n");
	assert!(markdown.contains("**Repository:** `monochange/monochange`"));
	assert!(markdown.contains("**Workflow:** `publish.yml`"));
	assert!(markdown.contains("**Environment:** `release`"));
	assert!(markdown.contains("**Setup:** `https://docs.npmjs.com/cli/v11/commands/npm-trust`"));

	let problem_report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Failed,
				package_publish::TrustedPublishingStatus::Disabled,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Blocked,
				package_publish::TrustedPublishingStatus::Disabled,
			),
		],
	};
	let markdown = render_package_publish_report_markdown(&problem_report, false).join("\n");
	assert!(markdown.contains("**Failed**\n- **`@scope/pkg`** `1.2.3` via `npm`"));
	assert!(markdown.contains("**Blocked**\n- **`@scope/pkg`** `1.2.3` via `npm`"));
}

/// A primary-tag run: one package planned, one pub.dev package deferred to
/// its release tag, and one package left to another tag's run.
fn tag_scoped_publish_report() -> package_publish::PackagePublishReport {
	let planned = sample_package_publish_outcome(
		package_publish::PackagePublishStatus::Planned,
		package_publish::TrustedPublishingStatus::Disabled,
	);
	let mut deferred = sample_package_publish_outcome(
		package_publish::PackagePublishStatus::DeferredToTag,
		package_publish::TrustedPublishingStatus::Disabled,
	);
	deferred.package = "dart_pkg".to_string();
	deferred.ecosystem = Ecosystem::Dart;
	deferred.registry = "pub_dev".to_string();
	deferred.version = "0.2.1".to_string();
	deferred.message = "pub.dev only accepts dart_pkg 0.2.1 from a run on `refs/tags/dart_pkg/v0.2.1`, and this run is on `refs/tags/v1.2.3`; publish it with `gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1`".to_string();
	deferred.trusted_publishing = monochange_publish::TrustedPublishingOutcome {
		status: package_publish::TrustedPublishingStatus::ManualActionRequired,
		repository: Some("owner/repo".to_string()),
		workflow: Some("publish.yml".to_string()),
		environment: None,
		setup_url: Some("https://pub.dev/packages/dart_pkg/admin".to_string()),
		message: "configure trusted publishing manually for `dart_pkg`".to_string(),
	};
	deferred.deferred_to = Some(monochange_publish::PublishDeferral {
		reason: monochange_publish::PublishDeferralReason::WrongRef,
		tag: "dart_pkg/v0.2.1".to_string(),
		git_ref: "refs/tags/dart_pkg/v0.2.1".to_string(),
		command: Some(
			"gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1".to_string(),
		),
	});
	let mut out_of_scope = sample_package_publish_outcome(
		package_publish::PackagePublishStatus::SkippedOutOfScope,
		package_publish::TrustedPublishingStatus::Disabled,
	);
	out_of_scope.package = "other_dart".to_string();
	out_of_scope.message = "not published in this run".to_string();

	package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![planned, deferred, out_of_scope],
	}
}

/// A run on a pub.dev package's own tag that a `release` event started.
fn wrong_event_publish_report() -> package_publish::PackagePublishReport {
	let mut deferred = sample_package_publish_outcome(
		package_publish::PackagePublishStatus::DeferredToTag,
		package_publish::TrustedPublishingStatus::Disabled,
	);
	deferred.package = "dart_pkg".to_string();
	deferred.ecosystem = Ecosystem::Dart;
	deferred.registry = "pub_dev".to_string();
	deferred.version = "0.2.1".to_string();
	deferred.message = "pub.dev only accepts dart_pkg 0.2.1 from a run that a `push` or `workflow_dispatch` event starts, and a `release` event started this run on `refs/tags/dart_pkg/v0.2.1`; start the publish workflow from tag pushes (`on: push`) instead of `on: release`, or publish it with `gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1`".to_string();
	deferred.deferred_to = Some(monochange_publish::PublishDeferral {
		reason: monochange_publish::PublishDeferralReason::WrongEvent,
		tag: "dart_pkg/v0.2.1".to_string(),
		git_ref: "refs/tags/dart_pkg/v0.2.1".to_string(),
		command: Some(
			"gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1".to_string(),
		),
	});

	package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![deferred],
	}
}

#[test]
fn render_package_publish_reports_say_what_to_change_for_a_wrong_event() {
	let report = wrong_event_publish_report();
	let text = render_publish_text(&report, false);
	assert!(
		text.contains("instead of `on: release`"),
		"text report:\n{text}"
	);
	assert!(text.contains("gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1"));

	let markdown = render_package_publish_report_markdown(&report, false).join("\n");
	assert!(
		markdown.contains("- **`dart_pkg`** `0.2.1` via `pub_dev` — pub.dev only accepts dart_pkg 0.2.1 from a run that a `push` or `workflow_dispatch` event starts"),
		"markdown report:\n{markdown}"
	);
	assert!(markdown.contains("instead of `on: release`"));

	let rendered =
		render_publish_command_json(OutputFormat::Json, Some(&report), None).expect("json");
	let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
	assert_eq!(
		value["package_publish"]["packages"][0]["deferred_to"]["reason"],
		"wrong_event"
	);
}

#[test]
fn deferrals_no_run_picks_up_warn_until_the_workflow_is_fixed() {
	let report = wrong_event_publish_report();
	let (outcome, line) = package_publish_outcome_line(&report, report.summary());
	assert_eq!(outcome, Outcome::Warning);
	assert!(
		line.ends_with(
			"; 1 package left unpublished, and no run publishes it until the workflow's trigger or `GITHUB_REF` is fixed"
		),
		"{line}"
	);

	let mut unknown_ref = wrong_event_publish_report();
	let mut second = unknown_ref.packages[0].clone();
	second.package = "tools_dart".to_string();
	unknown_ref.packages.push(second);
	for package in &mut unknown_ref.packages {
		if let Some(deferral) = package.deferred_to.as_mut() {
			deferral.reason = monochange_publish::PublishDeferralReason::UnknownRef;
		}
	}
	let (outcome, line) = package_publish_outcome_line(&unknown_ref, unknown_ref.summary());
	assert_eq!(outcome, Outcome::Warning);
	assert!(
		line.ends_with(
			"; 2 packages left unpublished, and no run publishes them until the workflow's trigger or `GITHUB_REF` is fixed"
		),
		"{line}"
	);

	// A deferral to another tag's run is picked up by that run.
	let tag_scoped = tag_scoped_publish_report();
	let (outcome, line) = package_publish_outcome_line(&tag_scoped, tag_scoped.summary());
	assert_ne!(outcome, Outcome::Warning, "{line}");
	assert!(!line.contains("left unpublished"), "{line}");

	// A failure stays a failure.
	let mut failed = wrong_event_publish_report();
	failed.packages.push(sample_package_publish_outcome(
		package_publish::PackagePublishStatus::Failed,
		package_publish::TrustedPublishingStatus::Disabled,
	));
	let (outcome, line) = package_publish_outcome_line(&failed, failed.summary());
	assert_eq!(outcome, Outcome::Failure);
	assert!(!line.contains("left unpublished"), "{line}");

	// Only rows with `status == DeferredToTag` count as stranded deferrals.
	let mut non_deferred_status = wrong_event_publish_report();
	non_deferred_status.packages[0].status = package_publish::PackagePublishStatus::Published;
	let (outcome, line) =
		package_publish_outcome_line(&non_deferred_status, non_deferred_status.summary());
	assert_eq!(outcome, Outcome::Success);
	assert!(!line.contains("left unpublished"), "{line}");

	// When the workflow cannot be determined, `deferred_to.command` is omitted
	// from text, markdown, and JSON, and the report explains why.
	let mut unknown_workflow = tag_scoped_publish_report();
	unknown_workflow.packages[1].message = "pub.dev only accepts dart_pkg 0.2.1 from a run on `refs/tags/dart_pkg/v0.2.1`, and this run is on `refs/tags/v1.2.3`; dispatch its publish workflow on `dart_pkg/v0.2.1` (set `publish.trusted_publishing.workflow` or keep `GITHUB_WORKFLOW_REF` so monochange can render the dispatch command)".to_string();
	if let Some(deferral) = unknown_workflow.packages[1].deferred_to.as_mut() {
		deferral.command = None;
	}
	let text = render_publish_text(&unknown_workflow, false);
	assert!(
		text.contains("publish.trusted_publishing.workflow"),
		"text:\n{text}"
	);
	assert!(
		!text.contains("Publish each one from its tag:"),
		"text:\n{text}"
	);
	let text_details = render_publish_text(&unknown_workflow, true);
	assert!(
		!text_details.contains("publish from tag:"),
		"details:\n{text_details}"
	);
	let markdown = render_package_publish_report_markdown(&unknown_workflow, false).join("\n");
	assert!(
		markdown.contains("publish.trusted_publishing.workflow"),
		"markdown:\n{markdown}"
	);
	let markdown_details =
		render_package_publish_report_markdown(&unknown_workflow, true).join("\n");
	assert!(
		!markdown_details.contains("Publish from tag:"),
		"markdown details:\n{markdown_details}"
	);
	let rendered = render_publish_command_json(OutputFormat::Json, Some(&unknown_workflow), None)
		.expect("json");
	let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
	assert!(
		value["package_publish"]["packages"][1]["deferred_to"]
			.get("command")
			.is_none(),
		"deferred_to should omit command when None: {value}"
	);
}

#[test]
fn render_package_publish_reports_list_deferred_packages_with_their_tag_runs() {
	let report = tag_scoped_publish_report();

	insta::assert_snapshot!(
		"publish_deferred_to_tag_text",
		render_publish_text(&report, false)
	);
	insta::assert_snapshot!(
		"publish_deferred_to_tag_markdown",
		render_package_publish_report_markdown(&report, false).join("\n")
	);

	let details = render_publish_text(&report, true);
	assert!(details.contains("dart_pkg 0.2.1 via pub_dev — deferred to its release tag"));
	assert!(details.contains("release tag: dart_pkg/v0.2.1 (refs/tags/dart_pkg/v0.2.1)"));
	assert!(details.contains(
		"publish from tag: gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1"
	));
	assert!(details.contains("other_dart 1.2.3 via npm — out of scope for this ref"));

	let markdown_details = render_package_publish_report_markdown(&report, true).join("\n");
	assert!(markdown_details.contains("- **Release tag:** `dart_pkg/v0.2.1`"));
	assert!(markdown_details.contains(
		"- **Publish from tag:** `gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v0.2.1`"
	));
}

#[test]
fn render_publish_command_json_exposes_deferred_tag_runs() {
	let report = tag_scoped_publish_report();
	let rendered =
		render_publish_command_json(OutputFormat::Json, Some(&report), None).expect("json");
	let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
	let package_publish = &value["package_publish"];

	assert_eq!(package_publish["summary"]["planned"], 1);
	assert_eq!(package_publish["summary"]["deferred"], 1);
	assert_eq!(package_publish["summary"]["out_of_scope"], 1);
	insta::assert_json_snapshot!(
		"publish_deferred_to_tag_json_package",
		package_publish["packages"][1]
	);
	assert_eq!(
		package_publish["packages"][2]["status"],
		"skipped_out_of_scope"
	);
	assert!(package_publish["packages"][2].get("deferred_to").is_none());
}

#[test]
fn render_package_publish_reports_include_command_output_blocks() {
	let mut outcome = sample_package_publish_outcome(
		package_publish::PackagePublishStatus::Published,
		package_publish::TrustedPublishingStatus::Disabled,
	);
	outcome.command = Some("npm publish --access public".to_string());
	outcome.stdout = Some("published\nwith provenance".to_string());
	outcome.stderr = Some("npm notice package".to_string());
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: false,
		packages: vec![outcome],
	};

	let default = render_publish_text(&report, false);
	assert!(!default.contains("command: npm publish --access public"));
	assert!(!default.contains("stdout:"));
	assert!(!default.contains("stderr:"));

	let text = render_publish_text(&report, true);
	assert!(text.contains("command: npm publish --access public"));
	assert!(text.contains("stdout:\n      │ published\n      │ with provenance"));
	assert!(text.contains("stderr:\n      │ npm notice package"));

	let markdown = render_package_publish_report_markdown(&report, true).join("\n");
	assert!(markdown.contains("**Command:** `npm publish --access public`"));
	assert!(markdown.contains("**stdout:**\n  ```text\n  published\n  with provenance\n  ```"));
	assert!(markdown.contains("**stderr:**\n  ```text\n  npm notice package\n  ```"));
}

#[test]
fn resolve_command_output_supports_publish_rate_limit_reports_without_release_state() {
	let cli_command = CliCommandDefinition {
		name: "publish-plan".to_string(),
		help_text: Some("plan publish rate limits".to_string()),
		inputs: vec![monochange_core::CliInputDefinition {
			name: "format".to_string(),
			kind: CliInputKind::Choice,
			help_text: Some("Output format".to_string()),
			required: false,
			default: Some("text".to_string()),
			choices: vec!["text".to_string(), "json".to_string()],
			short: None,
		}],
		steps: vec![CliStepDefinition::PlanPublishRateLimits {
			name: Some("plan publish rate limits".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let mut context = cli_context();
	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
	context.rate_limit_report = Some(sample_rate_limit_report());

	let text = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit text output: {error}"));
	insta::assert_snapshot!("publish_rate_limits_text", text);

	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
	context.output_format = OutputFormat::Json;
	let json = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit json output: {error}"));
	assert!(json.contains("batches_required"));
	assert!(json.contains("publish_rate_limits"));

	context.last_step_inputs =
		BTreeMap::from([("ci".to_string(), vec!["github-actions".to_string()])]);
	context.output_format = OutputFormat::Text;
	let github = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit github snippet: {error}"));
	assert!(github.contains("jobs:"));
	assert!(github.contains("wait_seconds: 86400"));
	assert!(github.contains("monochange publish packages"));

	context.last_step_inputs = BTreeMap::from([("ci".to_string(), vec!["gitlab-ci".to_string()])]);
	let gitlab = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit gitlab snippet: {error}"));
	assert!(gitlab.contains("publish_batches:"));
	assert!(gitlab.contains("WAIT_SECONDS: \"86400\""));

	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
	context.rate_limit_report = Some(monochange_core::PublishRateLimitReport {
		dry_run: true,
		windows: Vec::new(),
		batches: Vec::new(),
		warnings: Vec::new(),
	});
	let empty = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("empty rate limit output: {error}"));
	assert_eq!(empty, "• No publish operations matched the current plan");

	let mut windows_without_batches = sample_rate_limit_report();
	windows_without_batches.batches.clear();
	context.rate_limit_report = Some(windows_without_batches);
	let no_batches = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit output without batches: {error}"));
	assert!(no_batches.contains("Publish rate limits"));
	assert!(!no_batches.contains("Planned batches"));
	assert!(
		no_batches.ends_with("\n\n▲ Planned 13 publish operations · 0 batches · 1 registry"),
		"{no_batches}"
	);

	let mut fits_one_window = sample_rate_limit_report();
	fits_one_window.warnings.clear();
	context.rate_limit_report = Some(fits_one_window);
	let fits = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit output without warnings: {error}"));
	assert!(
		fits.ends_with("\n\n✔ Planned 13 publish operations · 2 batches · 1 registry"),
		"{fits}"
	);
}

#[test]
fn normalize_when_expression_preserves_inequality_and_mid_token_bangs() {
	assert_eq!(
		normalize_when_expression("{{ flag != other }}"),
		"{{ flag != other }}"
	);
	assert_eq!(normalize_when_expression("{{ foo!bar }}"), "{{ foo!bar }}");
}

#[test]
fn publish_rate_limit_helpers_parse_package_filters_modes_and_ci_renderers() {
	assert_eq!(
		selected_package_ids(&BTreeMap::from([(
			"package".to_string(),
			vec!["core".to_string(), "web".to_string(), "core".to_string()],
		)])),
		BTreeSet::from(["core".to_string(), "web".to_string()])
	);
	assert_eq!(
		publish_rate_limit_mode_from_inputs(&BTreeMap::new())
			.unwrap_or_else(|error| panic!("default mode: {error}")),
		publish_rate_limits::PublishRateLimitMode::Publish
	);
	assert_eq!(
		publish_rate_limit_mode_from_inputs(&BTreeMap::from([(
			"mode".to_string(),
			vec!["placeholder".to_string()],
		)]))
		.unwrap_or_else(|error| panic!("placeholder mode: {error}")),
		publish_rate_limits::PublishRateLimitMode::Placeholder
	);
	assert_eq!(
		requested_ci_renderer(&BTreeMap::from([(
			"ci".to_string(),
			vec!["gitlab-ci".to_string()],
		)]))
		.unwrap_or_else(|error| panic!("ci renderer: {error}")),
		Some("gitlab-ci")
	);

	let mode_error = publish_rate_limit_mode_from_inputs(&BTreeMap::from([(
		"mode".to_string(),
		vec!["ship-it".to_string()],
	)]))
	.expect_err("expected invalid mode error");
	assert!(
		mode_error
			.to_string()
			.contains("unsupported publish plan mode `ship-it`")
	);

	let renderer_error = requested_ci_renderer(&BTreeMap::from([(
		"ci".to_string(),
		vec!["circleci".to_string()],
	)]))
	.expect_err("expected invalid renderer error");
	assert!(
		renderer_error
			.to_string()
			.contains("unsupported publish CI renderer `circleci`")
	);

	let snippet_error =
		render_publish_rate_limit_ci_snippet(&sample_rate_limit_report(), "circleci")
			.expect_err("expected unsupported snippet renderer error");
	assert!(
		snippet_error
			.to_string()
			.contains("unsupported publish CI renderer `circleci`")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_matches_uses_progress_format_from_environment_and_rejects_invalid_values() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	temp_env::async_with_vars([("MONOCHANGE_PROGRESS_FORMAT", Some("json"))], async {
		let (configuration, matches) = parse_validate_matches(tempdir.path());
		let step_matches = matches
			.subcommand_matches("step")
			.and_then(|matches| matches.subcommand_matches("discover"))
			.unwrap_or_else(|| panic!("step discover subcommand matches"));
		execute_matches(
			tempdir.path(),
			&configuration,
			"discover",
			step_matches,
			false,
		)
		.await
		.unwrap_or_else(|error| panic!("step discover with env progress format: {error}"));
	})
	.await;

	temp_env::async_with_vars([("MONOCHANGE_PROGRESS_FORMAT", Some("wat"))], async {
		let (configuration, matches) = parse_validate_matches(tempdir.path());
		let step_matches = matches
			.subcommand_matches("step")
			.and_then(|matches| matches.subcommand_matches("discover"))
			.unwrap_or_else(|| panic!("step discover subcommand matches"));
		let error = execute_matches(
			tempdir.path(),
			&configuration,
			"discover",
			step_matches,
			false,
		)
		.await
		.unwrap_err();
		assert_eq!(
			error.to_string(),
			"config error: unknown progress format `wat`; expected one of: auto, unicode, ascii, json"
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn configured_config_step_uses_generic_completion_without_config_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let cli_command = CliCommandDefinition {
		name: "configured-config".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::Config {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let output = execute_cli_command_with_options(
		tempdir.path(),
		&configuration,
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: true,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
			progress: None,
		},
	)
	.await
	.unwrap_or_else(|error| panic!("config command: {error}"));

	assert_eq!(output, "✔ configured-config completed · dry-run");
	assert!(!output.contains("project_root"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_suppresses_progress_for_interactive_command_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let cli_command = CliCommandDefinition {
		name: "interactive-command".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::Command {
			name: Some("run tui".to_string()),
			when: None,
			always_run: false,
			command: "printf 'tui output\\n'".to_string(),
			dry_run_command: None,
			show_progress: None,
			shell: ShellConfig::Default,
			id: None,
			variables: None,
			inputs: BTreeMap::from([("interactive".to_string(), CliStepInputValue::Boolean(true))]),
		}],
		dry_run: false,
	};

	let output = execute_cli_command_with_options(
		tempdir.path(),
		&configuration,
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
			progress: None,
		},
	)
	.await
	.unwrap_or_else(|error| panic!("interactive command: {error}"));

	// The step ran without streaming or capturing its output, and no
	// `running command` status line was emitted.
	assert!(output.starts_with("Log\n"));
	assert!(output.contains("ran `printf 'tui output\\n'`"));
	assert!(!output.contains("running command"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_with_options_reuses_prepared_release_artifact_for_versions() {
	let workspace_dir = prepared_release_git_workspace();
	let root = workspace_dir.path();
	let configuration = sample_configuration(root);
	let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let artifact_path = artifact_dir.path().join("prepared-release.json");
	save_prepared_release_execution(
		root,
		&configuration,
		&sample_prepared_release_with_versions(),
		&[],
		Some(artifact_path.as_path()),
		"release",
	)
	.await
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let output = execute_cli_command_with_options(
		root,
		&configuration,
		&default_cli_command("display-versions"),
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::from([("format".to_string(), vec!["json".to_string()])]),
			prepared_release_path: Some(artifact_path),
			progress_format: ProgressFormat::Auto,
			progress: None,
		},
	)
	.await
	.unwrap_or_else(|error| panic!("execute versions command: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("parse versions output: {error}"));

	assert_eq!(parsed["groups"]["sdk"], serde_json::json!("2.0.0"));
	assert_eq!(parsed["packages"]["core"], serde_json::json!("1.2.0"));
	assert_eq!(parsed["packages"]["web"], serde_json::json!("1.2.1"));
}

#[test]
fn record_skipped_and_failure_helpers_cover_silent_paths() {
	let cli_command = default_cli_command("validate");
	let step = CliStepDefinition::Validate {
		name: Some("validate".to_string()),
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	let mut context = cli_context();
	let mut progress = ProgressReporter::new(&cli_command, false, true, ProgressFormat::Auto);

	record_skipped_cli_step(&mut context, &step, 0, &mut progress, false);
	report_cli_step_failure(
		&mut progress,
		false,
		0,
		&step,
		Duration::from_millis(1),
		&MonochangeError::Config("boom".to_string()),
	);

	assert!(context.command_logs.is_empty());
}

#[rstest::rstest]
#[case::commit("CommitRelease")]
#[case::request("OpenReleaseRequest")]
#[tokio::test(flavor = "multi_thread")]
async fn legacy_preparation_requires_a_trusted_producer_before_follow_up(#[case] step_name: &str) {
	let workspace = prepared_release_git_workspace();
	let root = workspace.path();
	let configuration = sample_configuration(root);
	let artifact_path = root.join(".monochange/local/prepared-release.json");
	let prepared = sample_prepared_release();
	let mut producer = release_cli_command();
	producer.name = "nightly-publish".to_string();
	let manifest = build_release_manifest(&producer, &prepared, None);
	let record_path = write_release_record_file(root, None, &manifest).unwrap();
	let record_before = fs::read_to_string(&record_path).unwrap();
	save_prepared_release_execution(
		root,
		&configuration,
		&prepared,
		&[],
		Some(&artifact_path),
		&producer.name,
	)
	.await
	.unwrap();
	let mut artifact: serde_json::Value =
		serde_json::from_str(&fs::read_to_string(&artifact_path).unwrap()).unwrap();
	artifact
		.as_object_mut()
		.unwrap()
		.remove("originating_command");
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&artifact).unwrap(),
	)
	.unwrap();
	let loaded = crate::prepared_release_cache::load_prepared_release_execution(
		root,
		&configuration,
		Some(&artifact_path),
		true,
		false,
	)
	.await
	.unwrap()
	.unwrap();
	assert_eq!(loaded.originating_command, None);

	let mut context = cli_context();
	let error = ensure_prepared_release_for_consumer_step(
		root,
		&configuration,
		&mut context,
		Some(&artifact_path),
		true,
		false,
		step_name,
	)
	.await
	.unwrap_err();
	assert!(error.to_string().contains("originating command"));
	assert!(error.to_string().contains("trusted workspace inputs"));
	assert!(context.prepared_release.is_none());
	assert_eq!(fs::read_to_string(&record_path).unwrap(), record_before);

	// A different producer can establish a new trusted identity, but preparation
	// must preserve the existing record until repair is explicitly requested.
	execute_cli_command_with_options(
		root,
		&configuration,
		&default_cli_command("prepare-release"),
		ExecuteCliCommandOptions {
			dry_run: true,
			quiet: true,
			show_diff: false,
			inputs: format_input("json"),
			prepared_release_path: Some(artifact_path.clone()),
			progress_format: ProgressFormat::Auto,
			progress: None,
		},
	)
	.await
	.unwrap();
	ensure_prepared_release_for_consumer_step(
		root,
		&configuration,
		&mut context,
		Some(&artifact_path),
		true,
		false,
		step_name,
	)
	.await
	.unwrap();
	let expected = build_release_manifest(
		&default_cli_command("commit-release"),
		context.prepared_release.as_ref().unwrap(),
		context.originating_command.as_deref(),
	);
	assert_eq!(
		expected.command,
		default_cli_command("prepare-release").name
	);
	let error =
		crate::release_artifacts::validate_release_record_file(root, None, &expected, false)
			.unwrap_err();
	assert!(
		error
			.to_string()
			.contains("does not match expected content")
	);
	assert_eq!(fs::read_to_string(&record_path).unwrap(), record_before);
	assert_eq!(
		crate::release_artifacts::validate_release_record_file(root, None, &expected, true)
			.unwrap(),
		record_path
	);
	crate::release_artifacts::validate_release_record_file(root, None, &expected, false).unwrap();
}

#[test]
fn step_references_release_file_diffs_detects_all_supported_locations() {
	let from_when = CliStepDefinition::Validate {
		name: Some("validate".to_string()),
		when: Some("{{ file_diffs }}".to_string()),
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(step_references_release_file_diffs(&from_when));

	let mut inputs = BTreeMap::new();
	inputs.insert(
		"paths".to_string(),
		CliStepInputValue::List(vec!["{{ file_diffs }}".to_string()]),
	);
	let from_inputs = CliStepDefinition::PublishRelease {
		name: Some("publish".to_string()),
		when: None,
		always_run: false,
		inputs,
	};
	assert!(step_references_release_file_diffs(&from_inputs));

	let from_variables = CliStepDefinition::Command {
		name: Some("command".to_string()),
		when: None,
		always_run: false,
		command: "echo done".to_string(),
		dry_run_command: None,
		show_progress: None,
		shell: ShellConfig::Default,
		id: None,
		variables: Some(BTreeMap::from([(
			"file_diffs_payload".to_string(),
			CommandVariable::ChangedFiles,
		)])),
		inputs: BTreeMap::new(),
	};
	assert!(step_references_release_file_diffs(&from_variables));

	let without_file_diffs = CliStepDefinition::Command {
		name: Some("command".to_string()),
		when: None,
		always_run: false,
		command: "echo done".to_string(),
		dry_run_command: None,
		show_progress: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::from([("confirmed".to_string(), CliStepInputValue::Boolean(true))]),
	};
	assert!(!step_references_release_file_diffs(&without_file_diffs));
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_rate_limit_selected_package_ids_uses_readiness_artifact_for_publish_plans() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let artifact_path = tempdir.path().join("readiness.json");
	let report = publish_readiness::PublishReadinessReport {
		schema_version: 3,
		kind: "monochange.publishReadiness".to_string(),
		status: publish_readiness::PublishReadinessGlobalStatus::Ready,
		from: "prepared-release".to_string(),
		resolved_commit: "prepared-release".to_string(),
		record_commit: "prepared-release".to_string(),
		package_set_fingerprint: String::new(),
		input_fingerprint: "fnv1a64:3a84781749cb9027".to_string(),
		packages: Vec::new(),
		publish_order: Vec::new(),
		order_findings: Vec::new(),
	};
	let inputs = BTreeMap::from([(
		"readiness".to_string(),
		vec![artifact_path.display().to_string()],
	)]);
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&report)
			.unwrap_or_else(|error| panic!("serialize readiness artifact: {error}")),
	)
	.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));

	let selected = publish_rate_limit_selected_package_ids(
		tempdir.path(),
		&configuration,
		Some(&sample_prepared_release()),
		&inputs,
		publish_rate_limits::PublishRateLimitMode::Publish,
	)
	.await
	.unwrap_or_else(|error| panic!("selected packages from readiness: {error}"));

	assert!(selected.is_empty());
}

#[test]
fn selected_ecosystem_ids_rejects_unknown_ecosystem() {
	let inputs = BTreeMap::from([(
		"ecosystem".to_string(),
		vec!["unknown-ecosystem".to_string()],
	)]);
	let error = selected_ecosystem_ids(&inputs).expect_err("expected unknown ecosystem");
	assert!(error.to_string().contains("unknown ecosystem"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_always_run_steps_continue_after_failure() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let marker = tempdir.path().join("always-run-marker");
	let skipped_marker = tempdir.path().join("skipped-marker");
	let cli_command = CliCommandDefinition {
		name: "test".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("fail".to_string()),
				when: None,
				always_run: false,
				command: "exit 1".to_string(),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("always".to_string()),
				when: None,
				always_run: true,
				command: format!("touch {}", marker.display()),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("skip".to_string()),
				when: None,
				always_run: false,
				command: format!("touch {}", skipped_marker.display()),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
		],
		dry_run: false,
	};

	let configuration = sample_configuration(tempdir.path());
	let result = execute_cli_command_with_options(
		tempdir.path(),
		&configuration,
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
			progress: None,
		},
	);

	assert!(result.await.is_err());
	assert!(marker.exists(), "always_run step should have executed");
	assert!(
		!skipped_marker.exists(),
		"non-always_run step after failure should be skipped"
	);
}

#[test]
fn failed_command_details_repeat_only_the_tail_of_streamed_output() {
	let status = std::process::Command::new("sh")
		.args(["-c", "exit 101"])
		.status()
		.unwrap_or_else(|error| panic!("status: {error}"));
	let stderr = (1..=25)
		.map(|line| format!("line {line}"))
		.collect::<Vec<_>>()
		.join("\n");
	let output = PreparedProcessOutput {
		status,
		stdout: b"compiling\n".to_vec(),
		stderr: stderr.into_bytes(),
	};

	let streamed = ensure_command_succeeded(&output, "cargo test", true)
		.expect_err("a failing command should be reported")
		.to_string();
	assert!(streamed.contains("command `cargo test` failed: exit status: 101"));
	assert!(streamed.contains("stdout:\ncompiling"));
	assert!(streamed.contains("stderr (last 20 of 25 lines, full output above):\nline 6\n"));
	assert!(!streamed.contains("line 5\n"));

	let captured = render_process_failure_details(&output, None);
	assert!(captured.contains("stderr:\nline 1\n"));
	assert!(captured.ends_with("line 25"));
}

#[test]
fn release_automation_json_lists_command_step_results() {
	let command = release_cli_command();
	let mut context = cli_context();
	context.output_format = OutputFormat::Json;
	context.release_results = vec!["created release v1.2.3".to_string()];
	context.command_results = vec![CommandStepResult {
		step: "announce".to_string(),
		id: None,
		command: "printf done".to_string(),
		status: "succeeded",
		exit_code: Some(0),
		stdout: "done".to_string(),
		stderr: String::new(),
	}];

	let json = resolve_command_output(&command, &context, false, None)
		.unwrap_or_else(|error| panic!("release automation json: {error}"));
	let value: serde_json::Value =
		serde_json::from_str(&json).unwrap_or_else(|error| panic!("parse json: {error}\n{json}"));
	assert_eq!(value["commands"][0]["command"], "printf done");
	assert_eq!(value["releases"], serde_json::json!([]));

	context.command_results.clear();
	let json = resolve_command_output(&command, &context, false, None)
		.unwrap_or_else(|error| panic!("release automation json: {error}"));
	assert!(!json.contains("\"commands\""), "{json}");
}

#[test]
fn command_interpolation_decodes_back_to_single_arguments() {
	let mut context = cli_context();
	let mut prepared = sample_prepared_release();
	prepared.changeset_paths = vec![
		PathBuf::from(".changeset/benign-notes.md"),
		PathBuf::from(".changeset/name with spaces.md"),
	];
	prepared.changed_files = vec![PathBuf::from("Cargo.toml")];
	context.prepared_release = Some(prepared);

	let interpolated = interpolate_cli_command_command(
		&context,
		"git add {{ changesets }} {{ changed_files }}",
		None,
		&BTreeMap::new(),
	);

	let parts = shlex::split(&interpolated)
		.unwrap_or_else(|| panic!("split interpolated command: {interpolated}"));
	assert_eq!(
		parts,
		[
			"git",
			"add",
			".changeset/benign-notes.md",
			".changeset/name with spaces.md",
			"Cargo.toml",
		]
	);
}
