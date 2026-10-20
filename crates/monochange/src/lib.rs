#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![allow(unstable_features)]
#![cfg_attr(test, allow(unused_imports, unused_qualifications))]
#![feature(coverage_attribute)]
#![doc = include_str!("crate_docs.md")]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
#[cfg(feature = "mcp")]
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::ExitCode;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use analyze::render_analyze_report;
pub(crate) use monochange_changelog::ChangelogBuildContext;
pub(crate) use monochange_changelog::build_changelog_updates;
pub(crate) use monochange_changelog::render_jinja_template;
use monochange_classification::render_change_classification;
use monochange_classification::render_changeset_api_validation;

use crate::cli::classify_options_from_matches;
use crate::versioning_state::ResolvedReleaseValues;
pub mod changelog {
	pub use monochange_changelog::render_message_template;
}
pub use changeset_policy::affected_packages;
pub(crate) use changeset_policy::compute_changed_paths_since;
pub use changeset_policy::evaluate_changeset_policy;
pub(crate) use changeset_policy::is_changeset_markdown_path;
pub(crate) use changeset_policy::normalize_changed_path;
pub use changeset_policy::verify_changesets;
pub(crate) use changesets::*;
use clap::ValueEnum;
use clap::error::ErrorKind;
pub use cli::build_command;
use cli::build_command_with_cli;
use cli::cli_commands_for_root;
use cli::cli_commands_from_config;
use cli::current_dir_or_dot;
pub(crate) use cli_runtime::collect_cli_command_inputs;
use cli_runtime::execute_cli_command_with_progress;
use cli_runtime::execute_matches_with_progress;
pub(crate) use cli_runtime::parse_output_format;
use command_wizard::run_command_wizard;
use git_support::git_commit_paths;
use git_support::git_head_commit;
use git_support::git_stage_paths;
use migration_audit::run_migration_command;
#[cfg(feature = "cargo")]
use monochange_cargo::RustSemverProvider;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetRevision;
use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::CommitMessage;
use monochange_core::DEFAULT_CHANGELOG_OUTPUT;
use monochange_core::DEFAULT_CHANGELOG_STREAM;
use monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED;
use monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY;
use monochange_core::DEFAULT_RELEASE_TITLE_NAMESPACED;
use monochange_core::DEFAULT_RELEASE_TITLE_PRIMARY;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::FloatingTagFormat;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitFile;
use monochange_core::HostedCommitRef;
use monochange_core::HostedCommitRequest;
use monochange_core::HostedCommitResponse;
use monochange_core::HostedIssueCommentPlan;
use monochange_core::HostedReleaseRequest;
use monochange_core::HostedReleaseResponse;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::PackageRecord;
use monochange_core::PreparedChangeset;
use monochange_core::PreparedChangesetTarget;
use monochange_core::ReleaseBackend;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestCompatibilityEvidence;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestPlanDecision;
use monochange_core::ReleaseManifestPlanGroup;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleasePlan;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::ReleaseRecordProvider;
use monochange_core::ReleaseRecordTarget;
use monochange_core::RetargetOperation;
use monochange_core::RetargetProviderResult;
use monochange_core::RetargetResult;
use monochange_core::RetargetTagResult;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceChangeRequestOutcome;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::SourceReleaseOutcome;
use monochange_core::SourceReleaseRequest;
use monochange_core::VersionFormat;
use monochange_core::VersionedFileDefinition;
use monochange_core::materialize_dependency_edges;
use monochange_core::relative_to_root;
pub(crate) use monochange_core::root_relative;
#[cfg(feature = "forgejo")]
use monochange_forgejo as forgejo_provider;
#[cfg(feature = "gitea")]
use monochange_gitea as gitea_provider;
#[cfg(feature = "github")]
use monochange_github as github_provider;
#[cfg(feature = "gitlab")]
use monochange_gitlab as gitlab_provider;
use monochange_graph::build_release_plan;
use monochange_semver::CompatibilityProvider;
use monochange_semver::collect_assessments;
pub(crate) use release_artifacts::*;
pub use release_record::discover_release_record;
pub use release_record::execute_release_retarget;
pub use release_record::plan_release_retarget;
use release_record::render_release_record_discovery;
use release_record::render_release_tag_report;
pub use release_record::retarget_release;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use skill::SkillAction;
use skill::render_outcome as render_skill_outcome;
use skill::run_skill;
use subagents::SubagentOptions;
use subagents::run_subagents;
pub(crate) use versioned_files::*;
pub use workspace_ops::AddChangeFileRequest;
pub use workspace_ops::PlannedChangeFile;
use workspace_ops::PopulateWorkspaceResult;
pub use workspace_ops::add_change_file;
pub(crate) use workspace_ops::add_interactive_change_file;
pub(crate) use workspace_ops::discover_release_workspace;
pub use workspace_ops::discover_workspace;
use workspace_ops::init_workspace;
pub use workspace_ops::plan_change_file;
pub use workspace_ops::plan_release;
use workspace_ops::populate_workspace;
pub use workspace_ops::prepare_release;
pub(crate) use workspace_ops::prepare_release_execution_with_configuration;
pub(crate) use workspace_ops::push_change_target_markdown;

pub(crate) fn render_config_step(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	format: OutputFormat,
) -> MonochangeResult<String> {
	if matches!(format, OutputFormat::Text | OutputFormat::Markdown) {
		let config_path = monochange_config::config_path(root);
		let display_path = config_path.strip_prefix(root).unwrap_or(&config_path);
		let mut package_counts = BTreeMap::<&str, usize>::new();

		for package in &configuration.packages {
			*package_counts
				.entry(package.package_type.as_str())
				.or_default() += 1;
		}

		let package_breakdown = package_counts
			.into_iter()
			.map(|(ecosystem, count)| format!("{count} {ecosystem}"))
			.collect::<Vec<_>>()
			.join(", ");
		let packages = if package_breakdown.is_empty() {
			"0".to_string()
		} else {
			format!("{} ({package_breakdown})", configuration.packages.len())
		};

		return Ok(format!(
			"Workspace configuration\n\nPath: {}\nPackages: {packages}\nVersion groups: {}\nConfigured commands: {}\nChangelog streams: {}\n\nUse `--format json` to print the complete resolved configuration.",
			display_path.display(),
			configuration.groups.len(),
			configuration.cli.len(),
			configuration.changelog.streams.len(),
		));
	}

	let project_root = root
		.canonicalize()
		.unwrap_or_else(|_| root.to_path_buf())
		.display()
		.to_string();
	let config_path = monochange_config::config_path(root).display().to_string();
	let output = serde_json::json!({
		"project_root": project_root,
		"config_path": config_path,
		"config": configuration,
	});

	format.render_json_value(&output, "config step")
}

pub(crate) fn synthetic_step_command_definition(
	cli_command_name: &str,
) -> MonochangeResult<CliCommandDefinition> {
	let kebab = cli_command_name
		.strip_prefix("step ")
		.unwrap_or(cli_command_name);
	let step = monochange_core::all_step_variants()
		.into_iter()
		.find(|step| step.step_kebab_name() == kebab)
		.ok_or_else(|| {
			MonochangeError::Config(format!("unknown step command: {cli_command_name}"))
		})?;

	Ok(CliCommandDefinition {
		name: format!("step {kebab}"),
		help_text: step.name().map(ToString::to_string),
		inputs: step.step_inputs_schema(),
		steps: vec![step.with_inherited_step_inputs()],
		dry_run: false,
	})
}

mod analyze;

mod changeset_policy;
mod changesets;
mod cli;
mod cli_runtime;

mod cli_theme;
mod command_wizard;
mod git_support;
mod hosted_sources;
mod interactive;
mod jq_filter;
mod lint;
#[cfg(feature = "mcp")]
mod mcp;
mod migration_audit;
mod notes;
mod output;
mod package_publish;
mod prepared_release_cache;
mod publish_rate_limits;
mod publish_readiness;
mod release_artifacts;
mod release_branch_policy;
mod release_record;
mod skill;
mod subagents;
mod sync;
pub use sync::sync_workspace_versions;
mod tracing_setup;
mod trusted_publishing_readiness;
mod versioned_files;
mod versioning_state;
mod workspace_ops;

pub(crate) use prepared_release_cache::ensure_monochange_artifact_ignored;
pub(crate) use prepared_release_cache::maybe_load_prepared_release_execution;
pub(crate) use prepared_release_cache::save_prepared_release_execution;

/// Output renderer used by CLI commands and preview helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
	Text,
	Markdown,
	/// Pretty-printed JSON with no terminal styling.
	Json,
	/// Minified JSON with no whitespace and no terminal styling.
	JsonMin,
}

impl OutputFormat {
	/// Serialize a value as plain-text JSON for this output format.
	///
	/// [`OutputFormat::Json`] renders pretty-printed JSON while
	/// [`OutputFormat::JsonMin`] renders the same value minified with no
	/// whitespace. Neither variant injects colors or other terminal styling.
	///
	/// # Errors
	///
	/// Returns an error when the value cannot be serialized to JSON.
	#[must_use = "the rendered json result must be checked"]
	pub(crate) fn render_json_value<T>(self, value: &T, context: &str) -> MonochangeResult<String>
	where
		T: Serialize,
	{
		match self {
			Self::JsonMin => serde_json::to_string(value),
			_ => serde_json::to_string_pretty(value),
		}
		.map_err(|error| {
			MonochangeError::Config(format!("failed to render {context} as json: {error}"))
		})
	}
}

/// Semver bump accepted by `monochange change` and related APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ChangeBump {
	None,
	Patch,
	Minor,
	Major,
}

impl From<ChangeBump> for BumpSeverity {
	fn from(value: ChangeBump) -> Self {
		match value {
			ChangeBump::None => Self::None,
			ChangeBump::Patch => Self::Patch,
			ChangeBump::Minor => Self::Minor,
			ChangeBump::Major => Self::Major,
		}
	}
}

/// Repo-local subagent target understood by `monochange subagents`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentTarget {
	Claude,
	Vscode,
	Copilot,
	Pi,
	Codex,
	Cursor,
}

impl SubagentTarget {
	fn all() -> Vec<Self> {
		vec![
			Self::Claude,
			Self::Vscode,
			Self::Copilot,
			Self::Pi,
			Self::Codex,
			Self::Cursor,
		]
	}

	fn from_cli_value(value: &str) -> Option<Self> {
		match value {
			"claude" => Some(Self::Claude),
			"vscode" => Some(Self::Vscode),
			"copilot" => Some(Self::Copilot),
			"pi" => Some(Self::Pi),
			"codex" => Some(Self::Codex),
			"cursor" => Some(Self::Cursor),
			_ => None,
		}
	}
}

/// Output renderer for `monochange subagents`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentOutputFormat {
	Markdown,
	Text,
	Json,
	/// Minified JSON with no whitespace and no terminal styling.
	JsonMin,
}

fn parse_subagent_output_format_or_default(value: Option<&String>) -> SubagentOutputFormat {
	match value.map_or("text", String::as_str) {
		"json" => SubagentOutputFormat::Json,
		"json-min" => SubagentOutputFormat::JsonMin,
		"text" => SubagentOutputFormat::Text,
		_ => SubagentOutputFormat::Markdown,
	}
}

fn parse_subagent_targets<'value, I>(values: Option<I>) -> MonochangeResult<Vec<SubagentTarget>>
where
	I: IntoIterator<Item = &'value String>,
{
	let mut targets = Vec::new();

	for value in values.into_iter().flatten() {
		let Some(target) = SubagentTarget::from_cli_value(value) else {
			return Err(MonochangeError::Config(format!(
				"unsupported subagent target `{value}`"
			)));
		};

		if targets.contains(&target) {
			continue;
		}

		targets.push(target);
	}

	if targets.is_empty() {
		return Err(MonochangeError::Config(
			"expected at least one subagent target or `--all`".to_string(),
		));
	}

	Ok(targets)
}

/// Outward release target derived from a prepared release.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub tag_name: String,
	pub members: Vec<String>,
	pub rendered_title: String,
	pub rendered_changelog_title: String,
	/// Floating tag aliases moved to this target's release tag.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub floating_tags: Vec<FloatingTagFormat>,
}

/// Rendered changelog payload produced during release preparation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparedChangelog {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	/// Stable name of the changelog destination that produced this artifact.
	#[serde(default = "default_prepared_changelog_output")]
	pub output: String,
	/// Audience stream rendered into this artifact.
	#[serde(default = "default_prepared_changelog_stream")]
	pub stream: String,
	pub path: PathBuf,
	pub format: ChangelogFormat,
	pub notes: ReleaseNotesDocument,
	pub rendered: String,
}

fn default_prepared_changelog_output() -> String {
	DEFAULT_CHANGELOG_OUTPUT.to_owned()
}

fn default_prepared_changelog_stream() -> String {
	DEFAULT_CHANGELOG_STREAM.to_owned()
}

/// Structured result returned by release preparation APIs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparedRelease {
	pub plan: ReleasePlan,
	pub changeset_paths: Vec<PathBuf>,
	pub changesets: Vec<PreparedChangeset>,
	pub released_packages: Vec<String>,
	pub package_publications: Vec<PackagePublicationTarget>,
	pub version: Option<String>,
	pub group_version: Option<String>,
	pub release_targets: Vec<ReleaseTarget>,
	pub changed_files: Vec<PathBuf>,
	pub changelogs: Vec<PreparedChangelog>,
	pub updated_changelogs: Vec<PathBuf>,
	pub deleted_changesets: Vec<PathBuf>,
	pub dry_run: bool,
	/// Declared release values and display labels resolved at prepare time.
	pub versioning: ResolvedReleaseValues,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
struct PreparedFileDiff {
	path: PathBuf,
	diff: String,
	#[serde(skip_serializing)]
	display_diff: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct StepPhaseTiming {
	label: String,
	duration: Duration,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PreparedReleaseExecution {
	prepared_release: PreparedRelease,
	file_diffs: Vec<PreparedFileDiff>,
	phase_timings: Vec<StepPhaseTiming>,
	warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FileUpdate {
	path: PathBuf,
	content: Vec<u8>,
}

impl FileUpdate {
	fn from_manifest_update(update: monochange_core::ManifestFileUpdate) -> Self {
		Self {
			path: update.path,
			content: update.content,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ChangesetDiagnosticsReport {
	pub(crate) requested_changesets: Vec<PathBuf>,
	pub(crate) changesets: Vec<PreparedChangeset>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
struct RetargetReleaseReport {
	from: String,
	target: String,
	resolved_from_commit: String,
	record_commit: String,
	target_commit: String,
	distance: usize,
	is_descendant: bool,
	force: bool,
	dry_run: bool,
	sync_provider: bool,
	tags: Vec<String>,
	git_tag_results: Vec<RetargetTagResult>,
	provider_results: Vec<RetargetProviderResult>,
	status: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
struct CommitReleaseReport {
	subject: String,
	body: String,
	commit: Option<String>,
	/// Whether GitHub verified the commit. Only the hosted backend knows, so
	/// local commits and dry runs leave it unset.
	#[serde(skip_serializing_if = "Option::is_none")]
	verified: Option<bool>,
	tracked_paths: Vec<PathBuf>,
	dry_run: bool,
	status: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CliContext {
	root: PathBuf,
	dry_run: bool,
	quiet: bool,
	show_diff: bool,
	output_format: OutputFormat,
	inputs: BTreeMap<String, Vec<String>>,
	last_step_inputs: BTreeMap<String, Vec<String>>,
	prepared_release: Option<PreparedRelease>,
	prepared_file_diffs: Vec<PreparedFileDiff>,
	release_manifest_path: Option<PathBuf>,
	release_requests: Vec<SourceReleaseRequest>,
	release_results: Vec<String>,
	release_request: Option<SourceChangeRequest>,
	release_request_result: Option<String>,
	release_commit_report: Option<CommitReleaseReport>,
	package_publish_report: Option<package_publish::PackagePublishReport>,
	rate_limit_report: Option<monochange_core::PublishRateLimitReport>,
	issue_comment_plans: Vec<HostedIssueCommentPlan>,
	issue_comment_results: Vec<String>,
	changeset_policy_evaluation: Option<ChangesetPolicyEvaluation>,
	changeset_diagnostics: Option<ChangesetDiagnosticsReport>,
	retarget_report: Option<RetargetReleaseReport>,
	step_outputs: BTreeMap<String, CommandStepOutput>,
	command_logs: Vec<String>,
	command_results: Vec<CommandStepResult>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CommandStepOutput {
	stdout: String,
	stderr: String,
}

/// What one `Command` step did, reported under `commands` in JSON output.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct CommandStepResult {
	/// The step's `name`, or its command when the step is unnamed.
	step: String,
	/// The step's `id`, used to reference its output from later steps.
	id: Option<String>,
	command: String,
	/// `succeeded`, or `skipped` when a dry run did not run the command.
	status: &'static str,
	exit_code: Option<i32>,
	stdout: String,
	stderr: String,
}

const CHANGESET_DIR: &str = ".changeset";

/// Run the `monochange` CLI from the current process environment.
///
/// This initializes tracing, parses `std::env::args_os()`, executes the
/// matching subcommand, and prints any non-empty stdout payload unless
/// `--quiet` was requested.
#[must_use = "the run result must be checked"]
#[allow(clippy::large_futures)]
#[coverage(off)]
pub async fn run_from_env(bin_name: &'static str) -> MonochangeResult<()> {
	let log_level = extract_log_level_from_args();
	let args = std::env::args_os().collect::<Vec<_>>();
	let quiet = extract_quiet_from_args(args.iter().cloned());
	let mut progress = output::ProgressReporter::for_invocation(&args);
	tracing_setup::init_tracing(log_level.as_deref(), progress.warning_sink());
	let root = current_dir_or_dot();
	let output = run_with_args_in_dir_with_progress(bin_name, args, &root, &mut progress).await?;

	if !quiet && !output.is_empty() {
		println!("{output}");
	}

	Ok(())
}

/// Run a CLI binary from process arguments and return the process exit code.
///
/// Binary entrypoints choose the Tokio runtime before delegating here so `monochange`
/// and `monochange` share the same output and error handling.
#[coverage(off)]
#[must_use = "the process exit code must be returned"]
pub async fn run_cli_binary_from_env(bin_name: &'static str) -> ExitCode {
	let arguments = std::env::args_os().collect::<Vec<_>>();
	let command = cli_diagnostic_command(&arguments);
	let quiet = extract_quiet_from_args(arguments.iter().cloned());
	let log_level = extract_log_level_from_args();
	let mut progress = output::ProgressReporter::for_invocation(&arguments);
	tracing_setup::init_tracing(log_level.as_deref(), progress.warning_sink());
	let root = current_dir_or_dot();
	let result = Box::pin(run_with_args_in_dir_with_progress(
		bin_name,
		arguments,
		&root,
		&mut progress,
	))
	.await;

	match result {
		Ok(output) => {
			if !quiet && !output.is_empty() {
				println!("{output}");
			}

			ExitCode::SUCCESS
		}
		Err(error) => {
			let diagnostic =
				output::CliDiagnostic::from_error(&error, command.as_deref()).with_root(&root);
			let exit_code = diagnostic.exit_code();
			if !quiet {
				if let Some(output) = error.reported_output().filter(|output| !output.is_empty()) {
					println!("{output}");
				}

				progress.write_diagnostic(diagnostic);
			}

			ExitCode::from(exit_code)
		}
	}
}

fn extract_log_level_from_args() -> Option<String> {
	extract_log_level(std::env::args())
}

fn quiet_from_os_arg(arg: &OsString) -> bool {
	matches!(arg.to_str(), Some("--quiet" | "-q"))
}

fn is_root_help_request(args: &[OsString]) -> bool {
	let mut command_args = command_args_after_globals(args);
	matches!(command_args.next(), None | Some("-h" | "--help")) && command_args.next().is_none()
}

fn command_args_after_globals(args: &[OsString]) -> impl Iterator<Item = &str> {
	let mut skip_value = false;
	args.iter()
		.skip(1)
		.filter_map(|arg| arg.to_str())
		.skip_while(move |arg| {
			if skip_value {
				skip_value = false;
				return true;
			}
			if matches!(
				*arg,
				"--log-level" | "--format" | "--progress-format" | "--jq" | "--color"
			) {
				skip_value = true;
				return true;
			}
			matches!(*arg, "--quiet" | "-q" | "--verbose" | "-v") || arg.starts_with("--color=")
		})
}

fn cli_diagnostic_command(args: &[OsString]) -> Option<String> {
	let mut command_args =
		command_args_after_globals(args).filter(|argument| !argument.starts_with('-'));
	let first = command_args.next()?;
	let mut command = format!("monochange {first}");

	if matches!(first, "run" | "step" | "versions" | "lint" | "publish")
		&& let Some(second) = command_args.next()
	{
		command.push(' ');
		command.push_str(second);
	}

	Some(command)
}

fn extract_quiet_from_args<I>(args: I) -> bool
where
	I: IntoIterator<Item = OsString>,
{
	args.into_iter().any(|arg| quiet_from_os_arg(&arg))
}

fn extract_log_level<I>(args: I) -> Option<String>
where
	I: IntoIterator<Item = String>,
{
	let mut args = args.into_iter();

	while let Some(arg) = args.next() {
		if arg == "--log-level" {
			return args.next();
		}

		if let Some(value) = arg.strip_prefix("--log-level=") {
			return Some(value.to_string());
		}
	}

	None
}

fn selected_cli_output_format(matches: &clap::ArgMatches) -> MonochangeResult<OutputFormat> {
	if let Ok(Some(value)) = matches.try_get_one::<String>("format") {
		return parse_output_format(value);
	}

	if let Some((_, subcommand)) = matches.subcommand() {
		return selected_cli_output_format(subcommand);
	}

	Ok(OutputFormat::Text)
}

struct SnapshotRequest {
	path: Vec<String>,
	view: monochange_snapshot::SnapshotView,
}

fn parse_snapshot_request(args: &[OsString]) -> Option<SnapshotRequest> {
	let mut values = args
		.iter()
		.skip(1)
		.map(|arg| arg.to_string_lossy().to_string());
	let first = values.next()?;

	if first == "--snapshot" {
		return Some(SnapshotRequest {
			path: Vec::new(),
			view: monochange_snapshot::SnapshotView::Full,
		});
	}

	let mut path = Vec::new();

	if !first.starts_with('-') {
		path.push(first);
	}

	for value in values {
		if value == "--snapshot" {
			return Some(SnapshotRequest {
				path,
				view: monochange_snapshot::SnapshotView::Full,
			});
		}

		if !value.starts_with('-') {
			path.push(value);
		}
	}

	None
}

fn snapshot_view(value: &str) -> monochange_snapshot::SnapshotView {
	match value {
		"light" => monochange_snapshot::SnapshotView::Light,
		"index" => monochange_snapshot::SnapshotView::Index,
		_ => monochange_snapshot::SnapshotView::Full,
	}
}

fn render_snapshot_request(
	command: &clap::Command,
	request: &SnapshotRequest,
) -> MonochangeResult<String> {
	let snapshot = monochange_snapshot::snapshot_from_clap(command);
	let Some(snapshot) = snapshot.subtree(&request.path) else {
		return Err(MonochangeError::Config(format!(
			"snapshot command path not found: {}",
			request.path.join(" ")
		)));
	};
	let view = snapshot.view(request.view);
	view.to_json()
		.map_err(|error| MonochangeError::Config(format!("failed to render snapshot: {error}")))
}

fn skill_action_from_matches(matches: &clap::ArgMatches) -> SkillAction {
	match matches.subcommand() {
		Some(("read", read_matches)) => {
			SkillAction::Read {
				topic: read_matches.get_one::<String>("topic").cloned(),
			}
		}
		Some(("install", install_matches)) => {
			SkillAction::Install {
				destination: install_matches.get_one::<String>("dir").map(PathBuf::from),
				force: install_matches.get_flag("force"),
			}
		}
		_ => SkillAction::List,
	}
}

fn cli_snapshot_required_value(
	args: &[OsString],
	index: usize,
	flag: &str,
) -> MonochangeResult<String> {
	args.get(index)
		.and_then(|value| value.to_str())
		.map(ToString::to_string)
		.ok_or_else(|| MonochangeError::Config(format!("missing value for {flag}")))
}

fn read_cli_snapshot(path: &Path) -> MonochangeResult<monochange_snapshot::CommandSnapshot> {
	let contents = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to read CLI snapshot `{}`: {error}",
			path.display()
		))
	})?;
	serde_json::from_str(&contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse CLI snapshot `{}`: {error}",
			path.display()
		))
	})
}

fn render_cli_snapshot_classification(args: &[OsString]) -> MonochangeResult<Option<String>> {
	let Some(command) = args.get(1).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	let Some(subcommand) = args.get(2).and_then(|value| value.to_str()) else {
		return Ok(None);
	};

	if command != "change" || subcommand != "classify" {
		return Ok(None);
	}

	let mut before = None;
	let mut after = None;
	let mut format = OutputFormat::Text;
	let mut index = 3;

	while let Some(arg) = args.get(index).and_then(|value| value.to_str()) {
		match arg {
			"--cli-snapshot-before" => {
				index += 1;
				before = Some(PathBuf::from(cli_snapshot_required_value(
					args,
					index,
					"--cli-snapshot-before",
				)?));
			}
			"--cli-snapshot-after" => {
				index += 1;
				after = Some(PathBuf::from(cli_snapshot_required_value(
					args,
					index,
					"--cli-snapshot-after",
				)?));
			}
			"--format" => {
				index += 1;
				format =
					parse_output_format(&cli_snapshot_required_value(args, index, "--format")?)?;
			}
			_ => {}
		}

		index += 1;
	}

	let (Some(before), Some(after)) = (before, after) else {
		return Ok(None);
	};
	let before = read_cli_snapshot(&before)?;
	let after = read_cli_snapshot(&after)?;
	let report = monochange_snapshot::diff_command_snapshots(&before, &after);

	let output = match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			let mut output = format.render_json_value(&report, "CLI snapshot classification")?;
			output.push('\n');
			output
		}
		_ => render_cli_snapshot_classification_text(&report),
	};

	Ok(Some(output))
}

fn render_cli_snapshot_classification_text(
	report: &monochange_snapshot::SnapshotDiffReport,
) -> String {
	let mut output =
		format!("CLI snapshot recommendation: {:?}\n", report.recommendation).to_lowercase();

	for change in &report.changes {
		output.push_str(&format!("- {:?}: {}\n", change.severity, change.summary).to_lowercase());
	}

	output
}

/// Run `monochange versions sync`, or the read-only `monochange versions`
/// check when `check_only` is set.
fn run_versions_sync(
	root: &Path,
	matches: &clap::ArgMatches,
	check_only: bool,
) -> MonochangeResult<String> {
	let strategy_str = matches
		.get_one::<String>("strategy")
		.map_or("default", String::as_str);
	let strategy = sync::parse_strategy(strategy_str);
	let dry_run = check_only || matches.get_flag("dry-run");
	let format_str = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	let format = sync::parse_versions_output_format(format_str)?;
	let result = sync_workspace_versions(root, strategy, dry_run)?;

	Ok(sync::format_sync_result_for_cli(
		root, &result, dry_run, check_only, format,
	))
}

/// Execute the `monochange` CLI with an explicit argument iterator.
#[must_use = "the run result must be checked"]
pub async fn run_with_args<I>(bin_name: &'static str, args: I) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let root = current_dir_or_dot();
	run_with_args_in_dir(bin_name, args, &root).await
}

#[tracing::instrument(skip_all, fields(bin_name))]
/// Execute the `monochange` CLI against an explicit repository root.
///
/// This is primarily useful for tests and embedding, where the caller wants to
/// Render a clap `DisplayHelp`/`DisplayVersion` error with ANSI styling when
/// `colored` is true, or as plain text when false.
fn format_clap_error(error: &clap::Error, colored: bool) -> String {
	clap_styled_text(&error.render(), colored)
}

fn clap_styled_text(text: &clap::builder::StyledStr, colored: bool) -> String {
	if colored {
		text.ansi().to_string()
	} else {
		text.to_string()
	}
}

fn paint(text: &str, style: anstyle::Style, colored: bool) -> String {
	if colored {
		format!("{style}{text}{style:#}")
	} else {
		text.to_string()
	}
}

fn unexpected_argument_from_error(error: &clap::Error) -> Option<String> {
	let message = error.to_string();
	let (_, rest) = message.split_once("unexpected argument '")?;
	let (argument, _) = rest.split_once('\'')?;
	Some(argument.to_string())
}

fn custom_command_name_from_args(
	args: &[OsString],
	cli: &[CliCommandDefinition],
) -> Option<String> {
	args.iter()
		.skip(1)
		.filter_map(|arg| arg.to_str())
		.find_map(|arg| {
			cli.iter()
				.any(|command| command.name == arg)
				.then(|| arg.to_string())
		})
}

fn render_custom_command_argument_error(
	error: &clap::Error,
	cli_command: &CliCommandDefinition,
	colored: bool,
) -> String {
	let command = format!("monochange run {}", cli_command.name);
	let argument =
		unexpected_argument_from_error(error).unwrap_or_else(|| "the supplied option".to_string());
	let heading = paint("✖ Unexpected command input", cli_theme::error(), colored);
	let argument = paint(&argument, cli_theme::literal(), colored);
	let command = paint(&command, cli_theme::usage(), colored);
	let config_path = paint(
		&format!("[cli.{}]", cli_command.name),
		cli_theme::header(),
		colored,
	);
	let usage = paint("Usage", cli_theme::header(), colored);
	let fix = paint("How to fix", cli_theme::header(), colored);

	format!(
		"{heading}\n\n  Argument {argument} is not declared for custom command {command}.\n\n{usage}:\n  {}\n\n{fix}:\n  This command comes from {config_path} in monochange.toml.\n  Add a matching input there to make this option valid, for example:\n\n    [cli.{}]\n    inputs = [\n      {{ name = \"{}\", type = \"boolean\" }},\n    ]\n\n  Then run `monochange help {}` to confirm the option is listed.",
		cli::cli_command_usage(cli_command),
		cli_command.name,
		argument.trim_start_matches('-').replace('-', "_"),
		cli_command.name
	)
}

/// Turn a command-line parse failure into a diagnostic that keeps clap's own
/// explanation, usage line, and suggestions.
fn clap_usage_error(error: &clap::Error, command: Option<&str>) -> MonochangeError {
	let rendered = error.to_string();
	if matches!(
		error.kind(),
		ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
	) {
		// Keep the description, usage, and subcommand list; the option
		// reference is one `--help` away.
		let command = command.unwrap_or("monochange");
		let overview = rendered
			.split("\n\nOptions:")
			.next()
			.unwrap_or_default()
			.trim_end();
		return MonochangeError::Diagnostic(format!(
			"error: `{command}` needs a subcommand\n\n{overview}\n\nFor more information, try '{command} --help'."
		));
	}
	MonochangeError::Diagnostic(rendered.trim_end().to_string())
}

fn help_command_requested(args: &[OsString]) -> bool {
	command_args_after_globals(args)
		.find(|arg| !arg.starts_with('-'))
		.is_some_and(|arg| arg == "help")
}

fn command_help_request(args: &[OsString]) -> Option<&str> {
	let command_args = command_args_after_globals(args).collect::<Vec<_>>();
	let command_name = command_args.iter().find(|arg| !arg.starts_with('-'))?;

	if *command_name == "help" {
		return None;
	}

	command_args
		.iter()
		.any(|arg| matches!(*arg, "--help" | "-h"))
		.then_some(*command_name)
}

/// Commands whose results depend on monochange.toml, so running them without
/// one deserves a warning instead of a silent default plan.
fn command_reads_release_configuration(command_name: &str) -> bool {
	matches!(
		command_name,
		"check"
			| "next" | "next-versions"
			| "preview"
			| "prepare"
			| "create"
			| "affected"
			| "diagnose"
			| "config"
			| "versions"
			| "notes" | "publish"
			| "step"
	)
}

fn missing_configuration_warning(command_name: Option<&str>, root: &Path) -> Option<&'static str> {
	let reads_configuration = command_name.is_some_and(command_reads_release_configuration);
	(reads_configuration && !root.join("monochange.toml").exists()).then_some(
		"no monochange.toml found in this directory, so results use built-in defaults; run \
		 `monochange init` to create one",
	)
}

fn command_help_requires_workspace_configuration(command_name: &str) -> bool {
	matches!(command_name, "change")
}

fn clap_help_path_from_args(args: &[OsString]) -> Vec<String> {
	let command_args = command_args_after_globals(args).collect::<Vec<_>>();
	let Some(help_index) = command_args.iter().position(|arg| *arg == "help") else {
		return Vec::new();
	};
	let help_args = command_args.get(help_index + 1..).unwrap_or_default();

	if help_args.iter().any(|arg| matches!(*arg, "--help" | "-h")) {
		return vec!["help".to_string()];
	}

	help_args
		.iter()
		.filter(|arg| !arg.starts_with('-'))
		.map(|arg| (*arg).to_string())
		.collect()
}

fn render_clap_long_help(
	bin_name: &'static str,
	cli: &[CliCommandDefinition],
	path: &[String],
) -> String {
	let mut command = build_command_with_cli(bin_name, cli);
	command.build();

	let Some(target) = clap_command_at_path_mut(&mut command, path) else {
		return render_unknown_clap_help_path(bin_name, &command, path);
	};

	clap_styled_text(&target.render_long_help(), output::color::stdout_color())
}

fn clap_command_at_path_mut<'command>(
	command: &'command mut clap::Command,
	path: &[String],
) -> Option<&'command mut clap::Command> {
	let Some((segment, remaining)) = path.split_first() else {
		return Some(command);
	};

	let subcommand = command.find_subcommand_mut(segment)?;
	clap_command_at_path_mut(subcommand, remaining)
}

fn render_unknown_clap_help_path(
	bin_name: &'static str,
	command: &clap::Command,
	path: &[String],
) -> String {
	let requested = if path.is_empty() {
		bin_name.to_string()
	} else {
		format!("{bin_name} {}", path.join(" "))
	};

	let mut output = format!("error: unrecognized command path `{requested}`\n\n");
	output.push_str("Run one of these commands to inspect available help paths:\n");
	let _ = writeln!(output, "  {bin_name} --help");
	let _ = writeln!(output, "  {bin_name} help");

	let mut subcommands = command
		.get_subcommands()
		.filter(|subcommand| !subcommand.is_hide_set())
		.map(clap::Command::get_name)
		.collect::<Vec<_>>();
	subcommands.sort_unstable();

	if !subcommands.is_empty() {
		output.push_str("\nVisible top-level commands:\n");

		for subcommand in subcommands {
			let _ = writeln!(output, "  {subcommand}");
		}
	}

	output
}

fn render_help_command(
	bin_name: &'static str,
	args: &[OsString],
	cli: &[CliCommandDefinition],
) -> String {
	let path = clap_help_path_from_args(args);
	render_clap_long_help(bin_name, cli, &path)
}

fn format_populate_workspace_result(result: &PopulateWorkspaceResult) -> String {
	if result.added_commands.is_empty() {
		format!(
			"left {} unchanged; this version provides no default CLI workflow aliases. Use `monochange command` to define custom workflows",
			result.path.display()
		)
	} else {
		let mut message = format!(
			"updated {} and added {} default CLI commands: ",
			result.path.display(),
			result.added_commands.len()
		);

		for (index, command) in result.added_commands.iter().enumerate() {
			if index > 0 {
				message.push_str(", ");
			}

			message.push_str(command);
		}

		message
	}
}

/// control both the argv payload and the workspace root used for config loading
/// and command execution.
#[doc(hidden)]
#[allow(clippy::redundant_closure_for_method_calls)]
#[tracing::instrument(skip_all, fields(bin_name))]
pub async fn run_with_args_in_dir<I>(
	bin_name: &'static str,
	args: I,
	root: &Path,
) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let args = args.into_iter().collect::<Vec<_>>();
	let mut progress = output::ProgressReporter::for_invocation(&args);
	let result = Box::pin(run_with_args_in_dir_with_progress(
		bin_name,
		args,
		root,
		&mut progress,
	))
	.await;
	drop(progress);
	result
}

async fn run_with_args_in_dir_with_progress(
	bin_name: &'static str,
	args: Vec<OsString>,
	root: &Path,
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<String> {
	let verbose = output::terminal::verbose_requested(&args);
	let ascii = output::terminal::ProgressSettings::from_args(&args).format
		== output::ProgressFormat::Ascii;
	let color = output::color::ColorChoice::from_args(&args);
	output::color::with_color_choice(
		color,
		output::text::with_verbosity(
			verbose,
			output::text::with_ascii_symbols(
				ascii,
				Box::pin(dispatch_cli_args(bin_name, args, root, progress)),
			),
		),
	)
	.await
}

/// Runs one CLI invocation from raw arguments to rendered output.
///
/// Dispatch happens in three stages, each in its own helper so the order of decisions reads top to
/// bottom: requests that never need the workspace (version, built-in help, snapshots) are answered
/// first, then the configuration is loaded so commands defined in `monochange.toml` can be parsed,
/// and finally the selected subcommand runs and its output is shaped by `--quiet`, `--jq`, and the
/// output format.
async fn dispatch_cli_args(
	bin_name: &'static str,
	args: Vec<OsString>,
	root: &Path,
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<String> {
	if let Some(rendered) = render_config_free_request(bin_name, &args, root)? {
		return Ok(rendered);
	}

	let configuration = load_configuration_with_progress(root, &args, progress);
	let cli = cli_commands_from_config(&configuration);
	let quiet = extract_quiet_from_args(args.iter().cloned());

	let matches = match build_command_with_cli(bin_name, &cli).try_get_matches_from(args.clone()) {
		Ok(matches) => matches,
		Err(error) => return render_clap_parse_failure(&error, &args, &cli, configuration),
	};

	if let Some(warning) = missing_configuration_warning(matches.subcommand_name(), root) {
		progress.warning(warning);
	}

	let jq_expression = matches.get_one::<String>("jq").cloned();

	validate_jq_output_format(jq_expression.is_some(), &matches)?;

	let context = SubcommandContext {
		bin_name,
		root,
		cli: &cli,
		quiet,
	};
	// Boxed: every subcommand's future is held inline by the match, which puts
	// the dispatcher past clippy's large-futures budget.
	let rendered = Box::pin(run_cli_subcommand(
		&context,
		&matches,
		configuration,
		progress,
	))
	.await?;

	finish_cli_output(rendered, quiet, jq_expression, &matches)
}

/// Answers the requests that never need the workspace configuration.
///
/// Version output and help for built-in commands are parsed against the default command set before
/// any workspace loading, which keeps them responsive in repositories with costly package or glob
/// validation. `None` means the request needs the configuration.
fn render_config_free_request(
	bin_name: &'static str,
	args: &[OsString],
	root: &Path,
) -> MonochangeResult<Option<String>> {
	if is_root_help_request(args) {
		let cli = cli_commands_for_root(root);

		return Ok(Some(render_clap_long_help(bin_name, &cli, &[])));
	}

	if help_command_requested(args) {
		let cli = cli_commands_for_root(root);

		return Ok(Some(render_help_command(bin_name, args, &cli)));
	}

	if let Some(snapshot_request) = parse_snapshot_request(args) {
		let configuration = load_workspace_configuration(root);
		let cli = cli_commands_from_config(&configuration);
		let command = cli::build_snapshot_command(bin_name, &cli, &snapshot_request.path);

		return render_snapshot_request(&command, &snapshot_request).map(Some);
	}

	if let Some(rendered) = render_cli_snapshot_classification(args)? {
		return Ok(Some(rendered));
	}

	let base_command = build_command_with_cli(bin_name, &monochange_core::default_cli_commands());

	Ok(base_command
		.try_get_matches_from(args.iter().cloned())
		.err()
		.and_then(|error| render_config_free_clap_output(bin_name, args, root, &error)))
}

/// Renders version or command help that does not depend on the workspace configuration.
///
/// Help is rendered against the commands available at `root` rather than the default set, so a
/// configured command still shows its own inputs. Help for a command that reads the release
/// configuration is left to the slow path, which reports configuration errors properly.
fn render_config_free_clap_output(
	bin_name: &'static str,
	args: &[OsString],
	root: &Path,
	error: &clap::Error,
) -> Option<String> {
	if matches!(error.kind(), ErrorKind::DisplayVersion) {
		return Some(format_clap_error(error, output::color::stdout_color()));
	}

	let help_command = command_help_request(args);
	let needs_configuration =
		help_command.is_some_and(command_help_requires_workspace_configuration);
	let help_requested = matches!(error.kind(), ErrorKind::DisplayHelp) || help_command.is_some();

	if needs_configuration || !help_requested {
		return None;
	}

	let cli = cli_commands_for_root(root);

	build_command_with_cli(bin_name, &cli)
		.try_get_matches_from(args.iter().cloned())
		.err()
		.filter(|help_error| matches!(help_error.kind(), ErrorKind::DisplayHelp))
		.map(|help_error| format_clap_error(&help_error, output::color::stdout_color()))
}

/// Loads the workspace configuration, reporting it as a progress phase unless help was requested.
fn load_configuration_with_progress(
	root: &Path,
	args: &[OsString],
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<monochange_core::WorkspaceConfiguration> {
	let configuration_started = std::time::Instant::now();
	let show_configuration_progress = command_help_request(args).is_none();

	if show_configuration_progress {
		progress.phase_started("Loading workspace configuration");
	}

	let configuration = load_workspace_configuration(root);

	if show_configuration_progress {
		report_configuration_phase(progress, &configuration, configuration_started.elapsed());
	}

	configuration
}

/// Closes the configuration progress phase with its outcome.
fn report_configuration_phase(
	progress: &mut output::ProgressReporter,
	configuration: &MonochangeResult<monochange_core::WorkspaceConfiguration>,
	elapsed: Duration,
) {
	match configuration {
		Ok(_) => progress.phase_finished("Loaded workspace configuration", elapsed),
		Err(error) => {
			progress.phase_failed("Load workspace configuration", elapsed, &error.render());
		}
	}
}

/// Turns a failed parse against the configured commands into help output or a diagnostic.
fn render_clap_parse_failure(
	error: &clap::Error,
	args: &[OsString],
	cli: &[CliCommandDefinition],
	configuration: MonochangeResult<monochange_core::WorkspaceConfiguration>,
) -> MonochangeResult<String> {
	if matches!(
		error.kind(),
		ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
	) {
		return Ok(format_clap_error(error, output::color::stdout_color()));
	}

	if matches!(error.kind(), ErrorKind::UnknownArgument)
		&& let Some(command_name) = custom_command_name_from_args(args, cli)
		&& let Some(cli_command) = cli.iter().find(|command| command.name == command_name)
	{
		return Err(MonochangeError::Diagnostic(
			render_custom_command_argument_error(error, cli_command, output::color::stderr_color()),
		));
	}

	// Commands defined in monochange.toml cannot be parsed while the
	// file is broken, so the configuration error is the real cause.
	configuration?;

	Err(clap_usage_error(
		error,
		cli_diagnostic_command(args).as_deref(),
	))
}

/// Rejects `--jq` unless the command was asked for JSON output, because the filter reads JSON.
fn validate_jq_output_format(
	jq_requested: bool,
	matches: &clap::ArgMatches,
) -> MonochangeResult<()> {
	let rejects_jq = jq_requested
		&& !matches!(matches.subcommand_name(), Some("snapshot"))
		&& !matches!(
			selected_cli_output_format(matches)?,
			OutputFormat::Json | OutputFormat::JsonMin
		);

	if rejects_jq {
		return Err(MonochangeError::Config(
			"--jq requires explicit JSON output; add `--format json` or `--format json-min`"
				.to_string(),
		));
	}

	Ok(())
}

/// Applies the output options shared by every command: `--quiet`, `--jq`, and Markdown output.
fn finish_cli_output(
	rendered: String,
	quiet: bool,
	jq_expression: Option<String>,
	matches: &clap::ArgMatches,
) -> MonochangeResult<String> {
	if quiet {
		return Ok(String::new());
	}

	if let Some(expression) = jq_expression {
		return jq_filter::apply_jq_filter(&rendered, &expression);
	}

	// Markdown is meant to be pasted or rendered elsewhere, so it never
	// carries terminal styling, even when stdout is a terminal.
	if matches!(
		selected_cli_output_format(matches),
		Ok(OutputFormat::Markdown)
	) {
		return Ok(output::strip_terminal_controls(&rendered));
	}

	Ok(rendered)
}

/// What every subcommand handler needs from the dispatcher.
struct SubcommandContext<'context> {
	bin_name: &'static str,
	root: &'context Path,
	cli: &'context [CliCommandDefinition],
	quiet: bool,
}

/// Runs the parsed subcommand and returns its unshaped output.
///
/// `help` and `--snapshot` never reach this point, because [`render_config_free_request`] answers
/// any request that names them before the arguments are parsed. `--quiet` is applied once by
/// [`finish_cli_output`], so a handler only checks it when rendering has a side effect of its own. The configuration is only unwrapped by the commands that read it,
/// which is what lets `init`, `help`, and the other workspace-free commands run in a repository
/// whose `monochange.toml` is broken.
async fn run_cli_subcommand(
	context: &SubcommandContext<'_>,
	matches: &clap::ArgMatches,
	configuration: MonochangeResult<monochange_core::WorkspaceConfiguration>,
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<String> {
	let root = context.root;

	match matches.subcommand() {
		Some(("snapshot", snapshot_matches)) => run_snapshot_subcommand(context, snapshot_matches),
		Some(("init", init_matches)) => {
			let provider = init_matches
				.get_one::<String>("provider")
				.map(String::as_str);

			init_workspace(root, init_matches.get_flag("force"), provider)
				.map(|result| result.summary())
		}
		Some(("populate", _)) => {
			populate_workspace(root).map(|result| format_populate_workspace_result(&result))
		}
		Some(("command", _)) => run_command_wizard_for_cli(root, context.quiet),
		Some(("skill", skill_matches)) => run_skill_subcommand(context.quiet, skill_matches),
		Some(("subagents", subagent_matches)) => run_subagents_subcommand(root, subagent_matches),
		Some(("change", change_matches)) => {
			let classify_matches = change_matches
				.subcommand_matches("classify")
				.expect("clap requires the change classify subcommand");

			render_change_classification(root, &classify_options_from_matches(classify_matches)?)
		}
		Some(("api", api_matches)) => {
			let diff_matches = api_matches
				.subcommand_matches("diff")
				.expect("clap requires the api diff subcommand");

			render_change_classification(root, &classify_options_from_matches(diff_matches)?)
		}
		Some(("changeset", changeset_matches)) => {
			let validate_matches = changeset_matches
				.subcommand_matches("validate")
				.expect("clap requires the changeset validate subcommand");

			render_changeset_api_validation(root, &classify_options_from_matches(validate_matches)?)
		}
		Some(("analyze", analyze_matches)) => run_analyze_subcommand(root, analyze_matches).await,
		Some(("notes", notes_matches)) => {
			run_notes_subcommand(root, &configuration?, notes_matches).await
		}
		Some(("migrate", migrate_matches)) => {
			run_migration_command(root, context.quiet, migrate_matches)
		}
		#[cfg(feature = "mcp")]
		Some(("mcp", _)) => run_mcp_command_with(context.quiet, mcp::run_server).await,
		Some(("check", check_matches)) => {
			run_check_subcommand(root, &configuration?, check_matches, progress)
		}
		Some(("lint", lint_matches)) => lint::handle_lint_subcommand(root, lint_matches),
		Some(("versions", versions_matches)) => run_versions_subcommand(root, versions_matches),
		Some(("publish", publish_matches)) => {
			let (publish_command, publish_command_matches) = publish_matches
				.subcommand()
				.expect("clap requires a publish subcommand");
			let configuration = configuration?;
			let synthetic = cli::publish_step_command_definition(publish_command)?;

			run_synthetic_command(
				context,
				&configuration,
				&synthetic,
				publish_command_matches,
				false,
				progress,
			)
			.await
		}
		Some(("step", step_matches)) => {
			let (step_name, step_command_matches) = step_matches
				.subcommand()
				.expect("clap requires a step subcommand");
			let configuration = configuration?;
			let synthetic = synthetic_step_command_definition(step_name)?;

			run_synthetic_command(
				context,
				&configuration,
				&synthetic,
				step_command_matches,
				false,
				progress,
			)
			.await
		}
		Some((cli_command_name, cli_command_matches))
			if cli::top_level_step_alias(cli_command_name).is_some() =>
		{
			let alias = cli::top_level_step_alias(cli_command_name)
				.expect("alias was checked in match guard");
			let configuration = configuration?;
			let synthetic = cli::top_level_step_alias_command_definition(alias)
				.expect("top-level step alias target exists");

			run_synthetic_command(
				context,
				&configuration,
				&synthetic,
				cli_command_matches,
				alias.force_dry_run,
				progress,
			)
			.await
		}
		Some(("run", run_matches)) => {
			let (cli_command_name, cli_command_matches) = run_matches
				.subcommand()
				.expect("clap requires a run subcommand");

			run_configured_command(
				context,
				&configuration?,
				cli_command_name,
				cli_command_matches,
				progress,
			)
			.await
		}
		// Commands defined in monochange.toml, invoked as `monochange <name>`.
		Some((cli_command_name, cli_command_matches)) => {
			run_configured_command(
				context,
				&configuration?,
				cli_command_name,
				cli_command_matches,
				progress,
			)
			.await
		}
		None => Err(MonochangeError::Config("Usage: monochange".to_string())),
	}
}

/// Collects the repeated `command` path argument of `snapshot`.
fn command_values(matches: &clap::ArgMatches) -> Vec<String> {
	matches
		.get_many::<String>("command")
		.into_iter()
		.flatten()
		.cloned()
		.collect()
}

/// Reads the `--view` argument, defaulting to the full snapshot.
fn snapshot_view_from_matches(matches: &clap::ArgMatches) -> monochange_snapshot::SnapshotView {
	matches
		.get_one::<String>("view")
		.map_or(monochange_snapshot::SnapshotView::Full, |value| {
			snapshot_view(value)
		})
}

/// Lists, saves, or renders a CLI surface snapshot.
fn run_snapshot_subcommand(
	context: &SubcommandContext<'_>,
	matches: &clap::ArgMatches,
) -> MonochangeResult<String> {
	if matches.get_flag("list") {
		return monochange_classification::cli_surface::list_registered_clis(context.root);
	}

	let view = snapshot_view_from_matches(matches);

	if let Some(package_id) = matches.get_one::<String>("package") {
		return monochange_classification::cli_surface::run_package_snapshot(
			context.root,
			package_id,
			matches.get_flag("save"),
			view,
		);
	}

	let path = command_values(matches);

	render_snapshot_request(
		&cli::build_snapshot_command(context.bin_name, context.cli, &path),
		&SnapshotRequest { path, view },
	)
}

/// Runs a skill action; reading a topic writes it directly, so `--quiet` must skip rendering.
fn run_skill_subcommand(quiet: bool, matches: &clap::ArgMatches) -> MonochangeResult<String> {
	let outcome = run_skill(skill_action_from_matches(matches))?;

	if quiet {
		return Ok(String::new());
	}

	render_skill_outcome(outcome)
}

/// Generates subagent definitions for the selected targets.
fn run_subagents_subcommand(root: &Path, matches: &clap::ArgMatches) -> MonochangeResult<String> {
	let targets = if matches.get_flag("all") {
		SubagentTarget::all()
	} else {
		parse_subagent_targets(matches.get_many::<String>("target"))?
	};

	let options = SubagentOptions {
		targets,
		force: matches.get_flag("force"),
		dry_run: matches.get_flag("dry-run"),
		format: parse_subagent_output_format_or_default(matches.get_one::<String>("format")),
		generate_mcp: !matches.get_flag("no-mcp"),
	};

	run_subagents(root, &options)
}

/// Analyzes one package's API between release, main, and head refs.
async fn run_analyze_subcommand(
	root: &Path,
	matches: &clap::ArgMatches,
) -> MonochangeResult<String> {
	let package = matches
		.get_one::<String>("package")
		.map(String::as_str)
		.ok_or_else(|| MonochangeError::Config("missing analyze package".to_string()))?;
	let release_ref = matches.get_one::<String>("release-ref").map(String::as_str);
	let main_ref = matches.get_one::<String>("main-ref").map(String::as_str);
	let head_ref = matches.get_one::<String>("head-ref").map(String::as_str);
	let detection_level = matches
		.get_one::<String>("detection-level")
		.map_or("signature", String::as_str);
	let format = matches
		.get_one::<String>("format")
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;

	render_analyze_report(
		root,
		package,
		release_ref,
		main_ref,
		head_ref,
		detection_level,
		format,
	)
	.await
}

/// Renders one configured release-note output without changing any files.
async fn run_notes_subcommand(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	matches: &clap::ArgMatches,
) -> MonochangeResult<String> {
	let notes_output = matches
		.get_one::<String>("output")
		.expect("the required notes output is present")
		.as_str();
	let target = matches.get_one::<String>("target").map(String::as_str);
	let file = matches.get_one::<String>("file").map(Path::new);

	notes::render_notes(root, configuration, notes_output, target, file).await
}

/// Collects every value of a repeated string argument.
fn string_values(matches: &clap::ArgMatches, name: &str) -> Vec<String> {
	matches
		.get_many::<String>(name)
		.map(|values| values.cloned().collect())
		.unwrap_or_default()
}

/// Runs the workspace lint rules, optionally fixing what they find.
fn run_check_subcommand(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	matches: &clap::ArgMatches,
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<String> {
	progress.configure_named_command("check");

	let format = matches
		.get_one::<String>("format")
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;

	lint::run_check_command_with_configuration(
		root,
		configuration,
		matches.get_flag("fix"),
		&string_values(matches, "ecosystem"),
		&string_values(matches, "only"),
		format,
		matches.get_flag("verbose"),
		progress,
	)
}

/// Lists or synchronizes workspace versions; a bare `versions` checks them.
fn run_versions_subcommand(root: &Path, matches: &clap::ArgMatches) -> MonochangeResult<String> {
	match matches.subcommand() {
		Some(("list", list_matches)) => {
			let format_name = list_matches
				.get_one::<String>("format")
				.map_or("text", String::as_str);
			let format = sync::parse_versions_output_format(format_name)?;
			let inventory = sync::list_workspace_versions(root)?;

			Ok(sync::format_version_inventory_for_cli(&inventory, format)?)
		}
		// `sync` is the only other subcommand clap defines for `versions`.
		Some((_, sync_matches)) => run_versions_sync(root, sync_matches, false),
		None => run_versions_sync(root, matches, true),
	}
}

/// Runs a built-in step command (`publish`, `step`, or a top-level alias) with its parsed inputs.
async fn run_synthetic_command(
	context: &SubcommandContext<'_>,
	configuration: &monochange_core::WorkspaceConfiguration,
	synthetic: &CliCommandDefinition,
	command_matches: &clap::ArgMatches,
	force_dry_run: bool,
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<String> {
	let inputs = collect_cli_command_inputs(synthetic, command_matches);
	let dry_run = force_dry_run || command_matches.get_flag("dry-run");

	execute_cli_command_with_progress(
		context.root,
		configuration,
		synthetic,
		dry_run,
		context.quiet,
		inputs,
		progress,
	)
	.await
}

/// Runs a command defined in `monochange.toml`.
async fn run_configured_command(
	context: &SubcommandContext<'_>,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command_name: &str,
	cli_command_matches: &clap::ArgMatches,
	progress: &mut output::ProgressReporter,
) -> MonochangeResult<String> {
	// Boxed: the step-dispatch future grew past clippy's large-futures
	// budget once the hosted release backends joined the match.
	Box::pin(execute_matches_with_progress(
		context.root,
		configuration,
		cli_command_name,
		cli_command_matches,
		context.quiet,
		Some(progress),
	))
	.await
}

#[coverage(off)]
fn run_command_wizard_for_cli(root: &Path, quiet: bool) -> MonochangeResult<String> {
	let output = run_command_wizard(root)?;

	Ok(if quiet { String::new() } else { output })
}

#[cfg(feature = "mcp")]
async fn run_mcp_command_with<F, Fut>(_quiet: bool, run_server: F) -> MonochangeResult<String>
where
	F: FnOnce() -> Fut,
	Fut: Future<Output = ()>,
{
	run_server().await;
	Ok(String::new())
}

fn format_publish_state(publish_state: monochange_core::PublishState) -> &'static str {
	match publish_state {
		monochange_core::PublishState::Public => "public",
		monochange_core::PublishState::Private => "private",
		monochange_core::PublishState::Unpublished => "unpublished",
		monochange_core::PublishState::Excluded => "excluded",
		_ => "unknown",
	}
}

#[cfg(test)]
#[path = "__tests__/mod_tests.rs"]
mod tests;
