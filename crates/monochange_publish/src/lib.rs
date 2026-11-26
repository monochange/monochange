#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use monochange_core::DependencyEdge;
use monochange_core::DependencyKind;
use monochange_core::DependencySourceKind;
use monochange_core::Ecosystem;
use monochange_core::EcosystemSettings;
use monochange_core::GroupDefinition;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishFlow;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::PublishTimeoutSettings;
use monochange_core::RegistryKind;
use monochange_core::SourceConfiguration;
use monochange_core::TrustedPublishingSettings;
use monochange_core::WorkspaceConfiguration;
use monochange_core::default_publish_order_dependency_fields;
pub use reqwest::Client;
use reqwest::StatusCode;
use rustls::crypto::ring::default_provider as ring_provider;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use tempfile::TempDir;
use tracing::info;
use urlencoding::encode;

pub mod oidc;
mod rate_limit_errors;
mod rate_limits;

pub use oidc::ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV;
pub use oidc::ACTIONS_ID_TOKEN_REQUEST_URL_ENV;
pub use oidc::ActionsOidcContext;
pub use oidc::PUB_DEV_AUDIENCE;
pub use oidc::PUB_TOKEN_ENV_VAR;
pub use oidc::actions_id_token_request_url;
pub use oidc::actions_oidc_context;
pub use oidc::mint_actions_id_token;
pub use rate_limit_errors::CRATES_IO_RATE_LIMIT_DOCS;
pub use rate_limit_errors::PublishRateLimitHit;
pub use rate_limit_errors::detect_rate_limit_hit;
pub use rate_limit_errors::registry_display_name;
pub use rate_limits::PYPI_TRUSTED_PUBLISHERS_DOCS;
pub use rate_limits::plan_rate_limit_batches;
pub use rate_limits::plan_rate_limit_window;
pub use rate_limits::policies_for_rate_limit_operation;
pub use rate_limits::registry_rate_limit_policies;
pub use rate_limits::render_rate_limit_window;

pub const PLACEHOLDER_VERSION: &str = "0.0.0";

pub trait EcosystemProgressPresentation {
	fn progress_emoji(self) -> &'static str;
	fn progress_label(self) -> &'static str;
}

impl EcosystemProgressPresentation for Ecosystem {
	fn progress_emoji(self) -> &'static str {
		progress_emoji_for_label(self.as_str())
	}

	fn progress_label(self) -> &'static str {
		self.as_str()
	}
}

fn progress_emoji_for_label(label: &str) -> &'static str {
	match label {
		"cargo" => "🦀",
		"npm" => "📦",
		"deno" => "🦕",
		"dart" => "🎯",
		"python" => "🐍",
		"go" => "🐹",
		_ => "🌐",
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishProgressPackage {
	pub package_id: String,
	pub package_name: String,
	pub version: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
}

impl PublishProgressPackage {
	#[must_use]
	pub fn from_request(request: &PublishRequest) -> Self {
		Self {
			package_id: request.package_id.clone(),
			package_name: request.package_name.clone(),
			version: request.version.clone(),
			ecosystem: request.ecosystem,
			registry: request.registry.to_string(),
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PublishProgressEvent {
	RunStarted {
		mode: PackagePublishRunMode,
		dry_run: bool,
		total: usize,
		ecosystems: Vec<Ecosystem>,
	},
	RegistryCheckStarted(PublishProgressPackage),
	PackageStarted(PublishProgressPackage),
	PackageSkipped {
		package: PublishProgressPackage,
		message: String,
	},
	PackagePlanned(PublishProgressPackage),
	PackagePublished(PublishProgressPackage),
	PackageStaged(PublishProgressPackage),
	PackageFailed {
		package: PublishProgressPackage,
		message: String,
	},
	RunFinished {
		mode: PackagePublishRunMode,
		total: usize,
		published: usize,
		staged: usize,
		skipped: usize,
		failed: usize,
	},
}

pub trait PublishProgressReporter: Send + Sync {
	fn report(&self, event: PublishProgressEvent);
}

#[derive(Debug, Default)]
pub struct NoopPublishProgressReporter;

impl PublishProgressReporter for NoopPublishProgressReporter {
	fn report(&self, _event: PublishProgressEvent) {}
}

fn publish_progress_package(request: &PublishRequest) -> PublishProgressPackage {
	PublishProgressPackage::from_request(request)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SelectedReleasePublicationTargets {
	pub publication_targets: Vec<PackagePublicationTarget>,
	pub selected_packages: BTreeSet<String>,
}

pub fn select_release_publication_targets(
	groups: &[GroupDefinition],
	publication_targets: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
	selected_groups: &BTreeSet<String>,
	selected_ecosystems: &BTreeSet<Ecosystem>,
) -> SelectedReleasePublicationTargets {
	let mut publication_targets = publication_targets.to_vec();

	if !selected_ecosystems.is_empty() {
		publication_targets.retain(|target| selected_ecosystems.contains(&target.ecosystem));
	}

	let mut selected_packages = selected_packages.clone();

	for group_id in selected_groups {
		if let Some(group) = groups.iter().find(|group| group.id == *group_id) {
			selected_packages.extend(group.packages.iter().cloned());
		}
	}

	SelectedReleasePublicationTargets {
		publication_targets,
		selected_packages,
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePublishRunMode {
	Placeholder,
	Release,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePublishStatus {
	Planned,
	Published,
	/// The version was uploaded to the registry's staging queue and is not
	/// installable until a maintainer approves it (npm staged publishing).
	Staged,
	SkippedExisting,
	SkippedExternal,
	/// Not attempted: the registry would reject this package's upload from
	/// this run. The outcome's `deferred_to` names the reason, the release tag
	/// the package publishes from, and the command that starts that run.
	DeferredToTag,
	/// Not attempted: another run of the release publishes this package, the
	/// run on its home tag. The outcome's message names that run.
	SkippedOutOfScope,
	Blocked,
	Failed,
}

/// Why a [`PackagePublishStatus::DeferredToTag`] package was not uploaded,
/// and the run that publishes it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PublishDeferral {
	/// What about this run the registry would reject.
	pub reason: PublishDeferralReason,
	/// Release tag to run the publish workflow on, for example
	/// `my_dart_package/v1.2.3`.
	pub tag: String,
	/// The git ref the registry requires: `refs/tags/<tag>`.
	#[serde(rename = "ref")]
	pub git_ref: String,
	/// Command that dispatches the package's publish workflow on `tag`, or
	/// `None` when monochange cannot determine which workflow to dispatch.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub command: Option<String>,
}

/// What about a GitHub Actions run makes pub.dev reject a trusted upload.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishDeferralReason {
	/// The run is on another tag.
	WrongRef,
	/// The run's ref is not a tag: a branch or a pull request ref.
	WrongRefType,
	/// The run is on the package's tag, but an event other than `push` or
	/// `workflow_dispatch` started it.
	WrongEvent,
	/// The run has no `GITHUB_REF`, so its ref is unknown.
	UnknownRef,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustedPublishingStatus {
	Disabled,
	Planned,
	Configured,
	ManualActionRequired,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TrustedPublishingOutcome {
	pub status: TrustedPublishingStatus,
	pub repository: Option<String>,
	pub workflow: Option<String>,
	pub environment: Option<String>,
	pub setup_url: Option<String>,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackagePublishOutcome {
	pub package: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
	pub version: String,
	pub status: PackagePublishStatus,
	pub message: String,
	pub placeholder: bool,
	pub trusted_publishing: TrustedPublishingOutcome,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub command: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stdout: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stderr: Option<String>,
	/// Set when the registry refused the upload because of its rate limit.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rate_limit: Option<PublishRateLimitHit>,
	/// Set for [`PackagePublishStatus::DeferredToTag`]: why the package was
	/// not uploaded, the release tag it publishes from, and the command that
	/// starts that run.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub deferred_to: Option<PublishDeferral>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackagePublishReport {
	pub mode: PackagePublishRunMode,
	pub dry_run: bool,
	pub packages: Vec<PackagePublishOutcome>,
}

#[derive(Debug)]
pub struct PackagePublishFailure {
	error: MonochangeError,
	report: PackagePublishReport,
}

impl PackagePublishFailure {
	#[must_use]
	pub fn new(error: MonochangeError, report: PackagePublishReport) -> Self {
		Self { error, report }
	}

	#[must_use]
	pub fn report(&self) -> &PackagePublishReport {
		&self.report
	}

	#[must_use]
	pub fn into_report(self) -> PackagePublishReport {
		self.report
	}

	#[must_use]
	pub fn error(&self) -> &MonochangeError {
		&self.error
	}

	#[must_use]
	pub fn into_error(self) -> MonochangeError {
		self.error
	}

	#[must_use]
	pub fn into_parts(self) -> (MonochangeError, PackagePublishReport) {
		(self.error, self.report)
	}

	#[must_use]
	pub fn render(&self) -> String {
		self.error.render()
	}
}

impl std::fmt::Display for PackagePublishFailure {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.error.fmt(formatter)
	}
}

impl std::error::Error for PackagePublishFailure {}

pub type PackagePublishExecutionResult = Result<PackagePublishReport, PackagePublishFailure>;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackagePublishSummary {
	pub planned: usize,
	pub published: usize,
	#[serde(default)]
	pub staged: usize,
	pub already_exists: usize,
	pub blocked: usize,
	pub failed: usize,
	pub not_attempted: usize,
	/// Packages left for a run on their own release tag. Omitted from JSON
	/// when zero, so runs without tag-bound packages report as before.
	#[serde(default, skip_serializing_if = "is_zero")]
	pub deferred: usize,
	/// Packages that belong to a run on another ref. Omitted from JSON when
	/// zero.
	#[serde(default, skip_serializing_if = "is_zero")]
	pub out_of_scope: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(count: &usize) -> bool {
	*count == 0
}

impl PackagePublishSummary {
	/// Packages this run is responsible for: every package except the ones it
	/// leaves to another run (deferred or out of scope).
	#[must_use]
	pub const fn this_run(self) -> usize {
		self.total() - self.deferred - self.out_of_scope
	}

	#[must_use]
	pub const fn total(self) -> usize {
		self.planned
			+ self.published
			+ self.staged
			+ self.already_exists
			+ self.blocked
			+ self.failed
			+ self.not_attempted
			+ self.deferred
			+ self.out_of_scope
	}
}

impl PackagePublishReport {
	#[must_use]
	pub fn summary(&self) -> PackagePublishSummary {
		let mut summary = PackagePublishSummary {
			planned: 0,
			published: 0,
			staged: 0,
			already_exists: 0,
			blocked: 0,
			failed: 0,
			not_attempted: 0,
			deferred: 0,
			out_of_scope: 0,
		};

		for outcome in &self.packages {
			match outcome.status {
				PackagePublishStatus::Planned => summary.planned += 1,
				PackagePublishStatus::Published => summary.published += 1,
				PackagePublishStatus::Staged => summary.staged += 1,
				PackagePublishStatus::SkippedExisting => summary.already_exists += 1,
				PackagePublishStatus::SkippedExternal => summary.not_attempted += 1,
				PackagePublishStatus::DeferredToTag => summary.deferred += 1,
				PackagePublishStatus::SkippedOutOfScope => summary.out_of_scope += 1,
				PackagePublishStatus::Blocked => summary.blocked += 1,
				PackagePublishStatus::Failed => summary.failed += 1,
			}
		}

		debug_assert_eq!(self.packages.len(), summary.total());
		summary
	}
}

#[must_use]
pub fn disabled_trust_outcome() -> TrustedPublishingOutcome {
	TrustedPublishingOutcome {
		status: TrustedPublishingStatus::Disabled,
		repository: None,
		workflow: None,
		environment: None,
		setup_url: None,
		message: "trusted publishing disabled".to_string(),
	}
}

#[must_use]
pub fn failed_publish_outcome(
	mode: PackagePublishRunMode,
	request: &PublishRequest,
	message: String,
) -> PackagePublishOutcome {
	PackagePublishOutcome {
		package: request.package_id.clone(),
		ecosystem: request.ecosystem,
		registry: request.registry.to_string(),
		version: request.version.clone(),
		status: PackagePublishStatus::Failed,
		message,
		placeholder: mode == PackagePublishRunMode::Placeholder,
		trusted_publishing: disabled_trust_outcome(),
		command: None,
		stdout: None,
		stderr: None,
		rate_limit: None,
		deferred_to: None,
	}
}

/// Outcome for a release package this run leaves to another run.
///
/// `trusted_publishing` is the trust plan for the run that will publish the
/// package, so the report shows what that run needs instead of claiming
/// trusted publishing is disabled.
fn not_attempted_outcome(
	request: &PublishRequest,
	status: PackagePublishStatus,
	message: String,
	trusted_publishing: TrustedPublishingOutcome,
	deferred_to: Option<PublishDeferral>,
) -> PackagePublishOutcome {
	PackagePublishOutcome {
		package: request.package_id.clone(),
		ecosystem: request.ecosystem,
		registry: request.registry.to_string(),
		version: request.version.clone(),
		status,
		message,
		placeholder: false,
		trusted_publishing,
		command: None,
		stdout: None,
		stderr: None,
		rate_limit: None,
		deferred_to,
	}
}

fn append_publish_failure_outcomes(
	outcomes: &mut Vec<PackagePublishOutcome>,
	remaining_requests: &[PublishRequest],
	mode: PackagePublishRunMode,
	failed_request: &PublishRequest,
	message: String,
	progress: &dyn PublishProgressReporter,
) {
	progress.report(PublishProgressEvent::PackageFailed {
		package: publish_progress_package(failed_request),
		message: message.clone(),
	});
	outcomes.push(failed_publish_outcome(mode, failed_request, message));
	append_skipped_after_failure_outcomes(
		outcomes,
		remaining_requests,
		mode,
		failed_request,
		progress,
	);
}

fn empty_publish_report(
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
) -> PackagePublishReport {
	PackagePublishReport {
		mode,
		dry_run,
		packages: requests
			.iter()
			.map(|request| {
				PackagePublishOutcome {
					package: request.package_id.clone(),
					ecosystem: request.ecosystem,
					registry: request.registry.to_string(),
					version: request.version.clone(),
					status: PackagePublishStatus::Blocked,
					message: "not attempted because publishing could not start".to_string(),
					placeholder: mode == PackagePublishRunMode::Placeholder,
					trusted_publishing: disabled_trust_outcome(),
					command: None,
					stdout: None,
					stderr: None,
					rate_limit: None,
					deferred_to: None,
				}
			})
			.collect(),
	}
}

fn append_skipped_after_failure_outcomes(
	outcomes: &mut Vec<PackagePublishOutcome>,
	requests: &[PublishRequest],
	mode: PackagePublishRunMode,
	failed_request: &PublishRequest,
	progress: &dyn PublishProgressReporter,
) {
	for request in requests {
		// External packages and packages another run publishes keep their own
		// status: the failure here does not affect them.
		let (status, message, deferred_to) = match (mode, request.mode, &request.run_scope) {
			(PackagePublishRunMode::Release, PublishMode::External, _) => {
				(
					PackagePublishStatus::SkippedExternal,
					"package opted out of built-in publishing".to_string(),
					None,
				)
			}
			(PackagePublishRunMode::Release, _, PublishRunScope::OutOfScope { message }) => {
				(
					PackagePublishStatus::SkippedOutOfScope,
					message.clone(),
					None,
				)
			}
			(
				PackagePublishRunMode::Release,
				_,
				PublishRunScope::DeferredToTag { deferral, message },
			) => {
				(
					PackagePublishStatus::DeferredToTag,
					message.clone(),
					Some(deferral.clone()),
				)
			}
			_ => {
				(
					PackagePublishStatus::Blocked,
					format!(
						"not attempted because publishing {} {} failed earlier in this run",
						failed_request.package_name, failed_request.version
					),
					None,
				)
			}
		};
		progress.report(PublishProgressEvent::PackageSkipped {
			package: publish_progress_package(request),
			message: message.clone(),
		});
		outcomes.push(PackagePublishOutcome {
			package: request.package_id.clone(),
			ecosystem: request.ecosystem,
			registry: request.registry.to_string(),
			version: request.version.clone(),
			status,
			message,
			placeholder: mode == PackagePublishRunMode::Placeholder,
			trusted_publishing: disabled_trust_outcome(),
			command: None,
			stdout: None,
			stderr: None,
			rate_limit: None,
			deferred_to,
		});
	}
}

#[must_use]
pub fn planned_publish_message(mode: PackagePublishRunMode, request: &PublishRequest) -> String {
	match mode {
		PackagePublishRunMode::Placeholder => {
			format!(
				"would publish placeholder {} {} to {}",
				request.package_name, request.version, request.registry
			)
		}
		PackagePublishRunMode::Release => {
			if request.flow == PublishFlow::Staged {
				format!(
					"would stage {} {} on {}; the version stays non-installable until a maintainer approves it with 2FA",
					request.package_name, request.version, request.registry
				)
			} else {
				format!(
					"would publish {} {} to {}",
					request.package_name, request.version, request.registry
				)
			}
		}
	}
}

#[must_use]
pub fn non_empty_output(output: String) -> Option<String> {
	(!output.is_empty()).then_some(output)
}

pub fn reject_npm_token_environment(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<()> {
	let token_keys = forbidden_npm_token_env_keys(env_map);

	if token_keys.is_empty() {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"`{}` requires npm trusted publishing, but long-lived npm token environment variables are present: {}. Remove token-based npm credentials and publish from the configured CI/OIDC workflow, or set `publish.trusted_publishing = false` to opt out.",
		request.package_id,
		token_keys.join(", "),
	)))
}

#[must_use]
pub fn forbidden_npm_token_env_keys(env_map: &BTreeMap<String, String>) -> Vec<String> {
	env_map
		.keys()
		.filter(|key| is_forbidden_npm_token_env_key(key))
		.cloned()
		.collect()
}

#[must_use]
pub fn is_forbidden_npm_token_env_key(key: &str) -> bool {
	let lowercase_key = key.to_ascii_lowercase();
	matches!(
		key,
		"NPM_TOKEN" | "NODE_AUTH_TOKEN" | "NPM_CONFIG__AUTH_TOKEN" | "npm_config__authToken"
	) || (lowercase_key.starts_with("npm_config_")
		&& lowercase_key.contains("auth")
		&& lowercase_key.contains("token"))
}

pub fn enforce_release_attestation_prerequisites(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
	command_builder: &PublishCommandBuilder,
) -> MonochangeResult<()> {
	if !request.attestations.require_registry_provenance {
		return Ok(());
	}

	if !request.trusted_publishing.enabled {
		return Err(MonochangeError::Config(format!(
			"`{}` requires registry-native package provenance, but trusted publishing is disabled. Registry provenance is only enforceable for built-in publishing from a verifiable CI/OIDC identity; set `publish.trusted_publishing = true` or set `publish.attestations.require_registry_provenance = false` to opt out.",
			request.package_id,
		)));
	}

	let registry = PublishRegistry::Builtin(request.registry);
	let identity = detect_trusted_publishing_identity(env_map);
	let capability_message = trusted_publishing_capability_message(&registry, &identity);

	if !identity.is_verifiable_by_env() {
		return Err(MonochangeError::Config(format!(
			"`{}` requires registry-native package provenance from a verifiable CI/OIDC identity, but the current publishing context is local or unverifiable. {capability_message} Run `monochange publish packages` from the configured CI workflow or set `publish.attestations.require_registry_provenance = false` to opt out.",
			request.package_id,
		)));
	}

	let capability = provider_registry_trust_capability(&registry, identity.provider());

	if !capability.registry_native_provenance {
		return Err(MonochangeError::Config(format!(
			"`{}` cannot require registry-native package provenance for {} from {}. {capability_message} This registry/provider combination does not expose provenance monochange can require; set `publish.attestations.require_registry_provenance = false` to opt out or use an external publisher that enforces its own attestation policy.",
			request.package_id,
			request.registry,
			identity.provider().label(),
		)));
	}

	if !command_builder
		.adapter_for_registry(request.registry)
		.is_some_and(PublishAdapter::supports_provenance)
	{
		return Err(MonochangeError::Config(format!(
			"`{}` cannot require registry-native package provenance for {} yet. {capability_message} The registry supports provenance, but monochange's current built-in publisher for this ecosystem does not expose a publish command that can require it; set `publish.attestations.require_registry_provenance = false` to opt out or use an external publisher that enforces its own attestation policy.",
			request.package_id, request.registry,
		)));
	}

	Ok(())
}

#[must_use]
pub fn manual_setup_url(request: &PublishRequest) -> String {
	if request.registry == RegistryKind::CratesIo {
		format!("https://crates.io/crates/{}", encode(&request.package_name))
	} else if request.registry == RegistryKind::PubDev {
		format!("https://pub.dev/packages/{}/admin", request.package_name)
	} else if request.registry == RegistryKind::Jsr {
		format!("https://jsr.io/{}", request.package_name)
	} else if request.registry == RegistryKind::Pypi {
		format!(
			"https://pypi.org/manage/project/{}/settings/publishing/",
			request.package_name
		)
	} else if request.registry == RegistryKind::GoProxy {
		format!("https://pkg.go.dev/{}", go_module_path(request))
	} else if request.registry == RegistryKind::CocoaPodsTrunk {
		format!("https://cocoapods.org/pods/{}", request.package_name)
	} else if request.registry == RegistryKind::MavenCentral {
		maven_central_coordinate(request).map_or_else(
			|| "https://central.sonatype.com/".to_string(),
			|(group, artifact)| format!("https://central.sonatype.com/artifact/{group}/{artifact}"),
		)
	} else {
		format!(
			"https://www.npmjs.com/package/{}/access",
			request.package_name
		)
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishRequest {
	pub package_id: String,
	pub package_name: String,
	pub ecosystem: Ecosystem,
	pub manifest_path: PathBuf,
	pub package_root: PathBuf,
	pub registry: RegistryKind,
	pub package_manager: Option<String>,
	pub package_metadata: BTreeMap<String, String>,
	pub mode: PublishMode,
	pub flow: PublishFlow,
	pub version: String,
	pub placeholder: bool,
	pub trusted_publishing: TrustedPublishingSettings,
	pub attestations: PublishAttestationSettings,
	pub timeout: PublishTimeoutSettings,
	pub fail_on_duplicate: bool,
	pub placeholder_readme: String,
	/// The git tag monochange creates for this package version, or `None`
	/// when the package's release owner does not create tags.
	pub release_tag: Option<PublishReleaseTag>,
	/// Whether the current run publishes this request, set by
	/// [`assign_publish_run_scopes`]. Requests start as
	/// [`PublishRunScope::Publish`]; placeholder publishing ignores it.
	pub run_scope: PublishRunScope,
}

/// The git tag monochange creates for one package release.
///
/// Registries that bind trusted uploads to a git ref (pub.dev) accept the
/// package only from a publish run on this tag.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishReleaseTag {
	/// Tag name, for example `my_dart_package/v1.2.3`.
	pub name: String,
}

/// The tags one release creates.
///
/// [`plan_publish_run_scopes`] compares a run's tag with them: when the
/// release has tag-bound packages, each package publishes in the run on one
/// of these tags, and the primary tag's run publishes every package that is
/// not tag-bound.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct PublishReleaseTags {
	/// The release's primary tag (for example `v1.2.3`): the tag of the
	/// release owner with `version_format = "primary"`, when this release
	/// creates it.
	pub primary: Option<String>,
	/// Every tag the release creates, the primary tag included.
	pub tags: BTreeSet<String>,
}

impl PublishReleaseTag {
	/// The tag's full git ref, `refs/tags/<name>`.
	#[must_use]
	pub fn git_ref(&self) -> String {
		format!("refs/tags/{}", self.name)
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandSpec {
	pub program: String,
	pub args: Vec<String>,
	pub cwd: PathBuf,
	pub env: BTreeMap<String, String>,
	pub timeout: Option<Duration>,
}

const NPM_PUBLISH_OTP_METADATA_KEY: &str = "monochange:npm_publish_otp";
const PUBLISH_STREAM_OUTPUT_ENV_KEY: &str = "MONOCHANGE_PUBLISH_STREAM_OUTPUT";
const PUBLISH_STREAM_OUTPUT_METADATA_KEY: &str = "monochange:stream_output";
const PUBLISH_TIMEOUT_ERROR_PREFIX: &str = "monochange:publish-timeout";

pub fn set_npm_publish_otp_for_requests(requests: &mut [PublishRequest], otp: &str) {
	for request in requests
		.iter_mut()
		.filter(|request| request.registry == RegistryKind::Npm)
	{
		request
			.package_metadata
			.insert(NPM_PUBLISH_OTP_METADATA_KEY.to_string(), otp.to_string());
	}
}

/// Force the run-level `publish.fail_on_duplicate` policy onto every request.
///
/// A `false` value leaves the per-package configuration untouched so the
/// built-in publishing step only ever strengthens the policy, never disables it.
pub fn set_fail_on_duplicate_for_requests(
	requests: &mut [PublishRequest],
	fail_on_duplicate: bool,
) {
	if !fail_on_duplicate {
		return;
	}

	for request in requests {
		request.fail_on_duplicate = true;
	}
}

fn npm_publish_env(request: &PublishRequest) -> BTreeMap<String, String> {
	let mut env = BTreeMap::new();

	// npm reads `NPM_CONFIG_OTP`; pnpm 12 ignores `NPM_CONFIG_*` and reads
	// only `PNPM_CONFIG_*`, so set both to reach whichever CLI publishes.
	if let Some(otp) = request.package_metadata.get(NPM_PUBLISH_OTP_METADATA_KEY) {
		env.insert("NPM_CONFIG_OTP".to_string(), otp.clone());
		env.insert("PNPM_CONFIG_OTP".to_string(), otp.clone());
	}

	env
}

pub type PlaceholderManifestWriter =
	dyn Fn(&Path, &PublishRequest, &Path, Option<&SourceConfiguration>) -> MonochangeResult<()>;

pub type PublishReadinessChecker =
	dyn Fn(&Path, &PublishRequest) -> MonochangeResult<Option<String>>;

/// Environment-aware readiness checker: same contract as
/// [`PublishReadinessChecker`], but receives the publish executor's
/// environment map so CI identities do not leak between test and production
/// environments.
pub type EnvPublishReadinessChecker =
	dyn Fn(&Path, &PublishRequest, &BTreeMap<String, String>) -> MonochangeResult<Option<String>>;

#[derive(Default)]
pub struct PublishReadinessRegistry {
	checkers: Vec<(RegistryKind, Box<PublishReadinessChecker>)>,
	env_checkers: Vec<(RegistryKind, Box<EnvPublishReadinessChecker>)>,
}

impl PublishReadinessRegistry {
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	#[must_use]
	pub fn with_checker(
		mut self,
		registry: RegistryKind,
		checker: Box<PublishReadinessChecker>,
	) -> Self {
		self.checkers.push((registry, checker));
		self
	}

	pub fn push_checker(&mut self, registry: RegistryKind, checker: Box<PublishReadinessChecker>) {
		self.checkers.push((registry, checker));
	}

	/// Register an environment-aware checker. Environment-aware checkers run
	/// alongside plain checkers and receive the publish executor's
	/// environment map instead of reading the process environment.
	#[must_use]
	pub fn with_env_checker(
		mut self,
		registry: RegistryKind,
		checker: Box<EnvPublishReadinessChecker>,
	) -> Self {
		self.env_checkers.push((registry, checker));
		self
	}

	pub fn blocked_message(
		&self,
		root: &Path,
		request: &PublishRequest,
	) -> MonochangeResult<Option<String>> {
		self.blocked_message_with_env(root, request, &current_env_map())
	}

	/// Run only environment-independent checkers. Dry-run planning uses this
	/// so a missing CI workflow file never marks a package skipped; trusted
	/// publishing enforcement is a real-run concern handled by the preflight
	/// and the trust prerequisites.
	pub fn blocked_message_static(
		&self,
		root: &Path,
		request: &PublishRequest,
	) -> MonochangeResult<Option<String>> {
		for (_, checker) in self
			.checkers
			.iter()
			.filter(|(registry, _)| *registry == request.registry)
		{
			if let Some(message) = checker(root, request)? {
				return Ok(Some(message));
			}
		}
		Ok(None)
	}

	/// Run every checker for the request's registry; environment-aware
	/// checkers receive `env_map` (the publish executor's environment).
	pub fn blocked_message_with_env(
		&self,
		root: &Path,
		request: &PublishRequest,
		env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<Option<String>> {
		for (_, checker) in self
			.checkers
			.iter()
			.filter(|(registry, _)| *registry == request.registry)
		{
			if let Some(message) = checker(root, request)? {
				return Ok(Some(message));
			}
		}

		for (_, checker) in self
			.env_checkers
			.iter()
			.filter(|(registry, _)| *registry == request.registry)
		{
			// patch-coverage:ignore-start -- llvm-cov attributes the `?`
			// error-propagation region to a zero-count line even though the
			// env-checker error path is exercised end-to-end by
			// env_readiness_checker_error_fails_the_run_before_any_mutation
			// and sanity_env_checker_direct.
			if let Some(message) = checker(root, request, env_map)? {
				// patch-coverage:ignore-end
				return Ok(Some(message));
			}
		}
		// patch-coverage:ignore-start -- llvm-cov maps this Ok(None) region to
		// a zero-count line even though the no-block fall-through is covered
		// by env_checker_without_block_falls_through_to_none.
		Ok(None)
		// patch-coverage:ignore-end
	}
}

#[derive(Default)]
pub struct PlaceholderManifestWriterRegistry {
	writers: Vec<(RegistryKind, Box<PlaceholderManifestWriter>)>,
}

impl PlaceholderManifestWriterRegistry {
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	#[must_use]
	pub fn with_writer(
		mut self,
		registry: RegistryKind,
		writer: Box<PlaceholderManifestWriter>,
	) -> Self {
		self.writers.push((registry, writer));
		self
	}

	pub fn push_writer(&mut self, registry: RegistryKind, writer: Box<PlaceholderManifestWriter>) {
		self.writers.push((registry, writer));
	}

	/// Write a placeholder manifest through the registered ecosystem writer.
	///
	/// # Errors
	/// Returns an error when the registry does not support placeholder publishing
	/// or when the selected writer cannot create its manifest.
	pub fn write_manifest(
		&self,
		placeholder_dir: &Path,
		request: &PublishRequest,
		root: &Path,
		source: Option<&SourceConfiguration>,
	) -> MonochangeResult<()> {
		let (_, writer) = self
			.writers
			.iter()
			.find(|(registry, _)| *registry == request.registry)
			.ok_or_else(|| {
				MonochangeError::Config(format!(
					"placeholder publishing is unsupported for registry `{}`",
					request.registry
				))
			})?;
		writer(placeholder_dir, request, root, source)
	}
}

pub fn build_placeholder_directory(
	root: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	manifest_writers: &PlaceholderManifestWriterRegistry,
) -> MonochangeResult<TempDir> {
	let tempdir = tempfile::tempdir().map_err(|error| placeholder_tempdir_error(&error))?;
	fs::write(
		tempdir.path().join("README.md"),
		&request.placeholder_readme,
	)
	.map_err(|error| MonochangeError::Io(format!("failed to write placeholder README: {error}")))?;
	write_placeholder_license(root, tempdir.path())?;
	write_placeholder_changelog(tempdir.path(), request)?;

	manifest_writers.write_manifest(tempdir.path(), request, root, source)?;

	Ok(tempdir)
}

fn write_placeholder_license(root: &Path, placeholder_dir: &Path) -> MonochangeResult<()> {
	let license = ["LICENSE", "LICENSE.md"]
		.into_iter()
		.find_map(|file_name| fs::read_to_string(root.join(file_name)).ok())
		.unwrap_or_else(|| {
			"Placeholder package published by monochange. See the source repository for license terms.\n"
				.to_string()
		});
	fs::write(placeholder_dir.join("LICENSE"), license).map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder LICENSE: {error}"))
	})
}

fn write_placeholder_changelog(
	placeholder_dir: &Path,
	request: &PublishRequest,
) -> MonochangeResult<()> {
	fs::write(
		placeholder_dir.join("CHANGELOG.md"),
		format!("## {}\n\n- Placeholder release.\n", request.version),
	)
	.map_err(|error| MonochangeError::Io(format!("failed to write placeholder CHANGELOG: {error}")))
}

fn placeholder_tempdir_error(error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!("failed to create placeholder tempdir: {error}"))
}

#[must_use]
pub fn current_env_map() -> BTreeMap<String, String> {
	env::vars().collect()
}

pub trait PublishTrustHandler {
	fn trust_outcome_for_skip(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome;

	fn planned_trust_outcome(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome;

	fn enforce_release_trust_prerequisites(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<()>;
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_publish_requests_with_process(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
) -> MonochangeResult<PackagePublishReport> {
	let env_map = current_env_map();
	let endpoints = RegistryEndpoints::from_env();
	let client = registry_client()?;
	let mut executor = ProcessCommandExecutor::new(false);
	try_execute_publish_requests_with_progress(
		root,
		source,
		mode,
		dry_run,
		requests,
		&client,
		&endpoints,
		&env_map,
		&mut executor,
		command_builder,
		manifest_writers,
		readiness,
		trust_handler,
		&NoopPublishProgressReporter,
	)
	.await
	.map_err(PackagePublishFailure::into_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_publish_requests_with_process_and_progress(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
	progress: &dyn PublishProgressReporter,
) -> MonochangeResult<PackagePublishReport> {
	try_execute_publish_requests_with_process_and_progress(
		root,
		source,
		mode,
		dry_run,
		requests,
		command_builder,
		manifest_writers,
		readiness,
		trust_handler,
		progress,
	)
	.await
	.map_err(PackagePublishFailure::into_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn try_execute_publish_requests_with_process_and_progress(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
	progress: &dyn PublishProgressReporter,
) -> PackagePublishExecutionResult {
	let env_map = current_env_map();
	let endpoints = RegistryEndpoints::from_env();
	let client = registry_client().map_err(|error| {
		// patch-coverage:ignore-start -- registry client build failure requires a broken TLS/network environment.
		PackagePublishFailure::new(error, empty_publish_report(mode, dry_run, requests))
	})?;
	// patch-coverage:ignore-end
	let mut executor = ProcessCommandExecutor::new(false);
	try_execute_publish_requests_with_progress(
		root,
		source,
		mode,
		dry_run,
		requests,
		&client,
		&endpoints,
		&env_map,
		&mut executor,
		command_builder,
		manifest_writers,
		readiness,
		trust_handler,
		progress,
	)
	.await
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_publish_requests(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
) -> MonochangeResult<PackagePublishReport> {
	try_execute_publish_requests_with_progress(
		root,
		source,
		mode,
		dry_run,
		requests,
		client,
		endpoints,
		env_map,
		executor,
		command_builder,
		manifest_writers,
		readiness,
		trust_handler,
		&NoopPublishProgressReporter,
	)
	.await
	.map_err(PackagePublishFailure::into_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_publish_requests_with_progress(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
	progress: &dyn PublishProgressReporter,
) -> MonochangeResult<PackagePublishReport> {
	try_execute_publish_requests_with_progress(
		root,
		source,
		mode,
		dry_run,
		requests,
		client,
		endpoints,
		env_map,
		executor,
		command_builder,
		manifest_writers,
		readiness,
		trust_handler,
		progress,
	)
	.await
	.map_err(PackagePublishFailure::into_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn try_execute_publish_requests_with_progress(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
	progress: &dyn PublishProgressReporter,
) -> PackagePublishExecutionResult {
	// Each release request carries the scope its caller planned for this
	// run's git ref (see `assign_publish_run_scopes`), so a registry that
	// binds uploads to a tag ref never sees an upload it must reject.
	// Placeholder publishing registers names and is not ref-bound.
	let run_scopes = requests
		.iter()
		.map(|request| {
			if mode == PackagePublishRunMode::Release {
				request.run_scope.clone()
			} else {
				PublishRunScope::Publish
			}
		})
		.collect::<Vec<_>>();

	// Fail fast before any registry mutation: run every per-package readiness
	// checker up-front so a package that cannot publish aborts the run before
	// an earlier package has already been published. Packages this run leaves
	// to another ref are not checked: they publish elsewhere.
	if mode == PackagePublishRunMode::Release && !dry_run {
		for (request, scope) in requests.iter().zip(&run_scopes) {
			if *scope == PublishRunScope::MissingReleaseTag {
				return Err(PackagePublishFailure::new(
					MonochangeError::Config(missing_release_tag_message(&request.package_id, None)),
					PackagePublishReport {
						mode,
						dry_run,
						packages: Vec::new(),
					},
				));
			}

			if *scope != PublishRunScope::Publish {
				continue;
			}

			let blocked = readiness
				.blocked_message_with_env(root, request, env_map)
				.map_err(|error| {
					PackagePublishFailure::new(
						error,
						PackagePublishReport {
							mode,
							dry_run,
							packages: Vec::new(),
						},
					)
				})?;

			if let Some(message) = blocked {
				return Err(PackagePublishFailure::new(
					MonochangeError::Config(message),
					PackagePublishReport {
						mode,
						dry_run,
						packages: Vec::new(),
					},
				));
			}
		}
	}

	let ecosystems = requests
		.iter()
		.map(|request| request.ecosystem)
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect::<Vec<_>>();
	progress.report(PublishProgressEvent::RunStarted {
		mode,
		dry_run,
		total: requests.len(),
		ecosystems,
	});
	let mut outcomes = Vec::with_capacity(requests.len());
	let mut primary_error = None;

	for (request_index, (request, scope)) in requests.iter().zip(&run_scopes).enumerate() {
		let remaining_requests = requests.get(request_index + 1..).unwrap_or_default();

		// External-mode packages are only skipped during release publishing.
		// Placeholder publishing is a bootstrap utility that should work for all
		// publishable packages, regardless of who handles normal release publishes.
		// See: https://github.com/monochange/monochange/issues/542
		if request.mode == PublishMode::External && mode == PackagePublishRunMode::Release {
			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				"skipping external package"
			);
			progress.report(PublishProgressEvent::PackageSkipped {
				package: publish_progress_package(request),
				message: "package opted out of built-in publishing".to_string(),
			});
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::SkippedExternal,
				message: "package opted out of built-in publishing".to_string(),
				placeholder: false,
				trusted_publishing: disabled_trust_outcome(),
				command: None,
				stdout: None,
				stderr: None,
				rate_limit: None,
				deferred_to: None,
			});
			continue;
		}

		match scope {
			PublishRunScope::OutOfScope { message } => {
				info!(
					package_name = request.package_name,
					version = %request.version,
					registry = %request.registry,
					"skipping package another run publishes"
				);
				progress.report(PublishProgressEvent::PackageSkipped {
					package: publish_progress_package(request),
					message: message.clone(),
				});
				outcomes.push(not_attempted_outcome(
					request,
					PackagePublishStatus::SkippedOutOfScope,
					message.clone(),
					trust_handler.planned_trust_outcome(request, source, root, env_map),
					None,
				));
				continue;
			}
			// Real runs abort in the preflight before any registry request; a
			// dry run reports the package as blocked, also without a registry
			// request, so the plan shows what to fix.
			PublishRunScope::MissingReleaseTag => {
				let message = missing_release_tag_message(&request.package_id, None);
				progress.report(PublishProgressEvent::PackageSkipped {
					package: publish_progress_package(request),
					message: message.clone(),
				});
				outcomes.push(not_attempted_outcome(
					request,
					PackagePublishStatus::Blocked,
					message,
					trust_handler.planned_trust_outcome(request, source, root, env_map),
					None,
				));
				continue;
			}
			PublishRunScope::Publish | PublishRunScope::DeferredToTag { .. } => {}
		}

		info!(
			package_name = request.package_name,
			version = %request.version,
			registry = %request.registry,
			dry_run,
			mode = ?mode,
			"publishing package"
		);

		progress.report(PublishProgressEvent::RegistryCheckStarted(
			publish_progress_package(request),
		));

		let version_exists = match registry_version_exists(client, endpoints, request).await {
			Ok(version_exists) => version_exists,
			Err(error) => {
				let message = error.render();
				primary_error = Some(error);
				append_publish_failure_outcomes(
					&mut outcomes,
					remaining_requests,
					mode,
					request,
					message,
					progress,
				);
				break;
			}
		};

		if version_exists {
			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				"skipping already-published version"
			);
			let message = format!(
				"{} {} already exists on {}",
				request.package_name, request.version, request.registry
			);

			if request.fail_on_duplicate
				&& mode == PackagePublishRunMode::Release
				&& *scope == PublishRunScope::Publish
			{
				info!(
					package_name = request.package_name,
					version = %request.version,
					registry = %request.registry,
					"failing already-published version because publish.fail_on_duplicate is enabled"
				);
				let duplicate_message = format!(
					"{message} and `publish.fail_on_duplicate` rejects duplicate version publications"
				);

				primary_error = Some(MonochangeError::Config(duplicate_message.clone()));
				append_publish_failure_outcomes(
					&mut outcomes,
					remaining_requests,
					mode,
					request,
					duplicate_message,
					progress,
				);
				break;
			}

			progress.report(PublishProgressEvent::PackageSkipped {
				package: publish_progress_package(request),
				message: message.clone(),
			});
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::SkippedExisting,
				message,
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: trust_handler
					.trust_outcome_for_skip(request, source, root, env_map),
				command: None,
				stdout: None,
				stderr: None,
				rate_limit: None,
				deferred_to: None,
			});
			continue;
		}

		// Deferral comes after the version check, so a version another run
		// already published reads `already exists` instead of asking for a
		// dispatch that has nothing left to do.
		if let PublishRunScope::DeferredToTag { deferral, message } = scope {
			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				tag = deferral.tag,
				reason = ?deferral.reason,
				"deferring package to its release tag run"
			);
			progress.report(PublishProgressEvent::PackageSkipped {
				package: publish_progress_package(request),
				message: message.clone(),
			});
			outcomes.push(not_attempted_outcome(
				request,
				PackagePublishStatus::DeferredToTag,
				message.clone(),
				trust_handler.planned_trust_outcome(request, source, root, env_map),
				Some(deferral.clone()),
			));
			continue;
		}

		// The mid-loop check uses the static (environment-independent)
		// checkers for every mode: environment-aware trusted-publishing
		// checks already ran in the fail-fast preflight above, and dry-run
		// planning must not block on a missing CI workflow file.
		let blocked_message = if mode == PackagePublishRunMode::Release {
			match readiness.blocked_message_static(root, request) {
				Ok(message) => message,
				Err(error) => {
					let message = error.render();
					primary_error = Some(error);
					append_publish_failure_outcomes(
						&mut outcomes,
						remaining_requests,
						mode,
						request,
						message,
						progress,
					);
					break;
				}
			}
		} else {
			None
		};

		if let Some(message) = blocked_message {
			if dry_run {
				progress.report(PublishProgressEvent::PackageSkipped {
					package: publish_progress_package(request),
					message: message.clone(),
				});
				outcomes.push(PackagePublishOutcome {
					package: request.package_id.clone(),
					ecosystem: request.ecosystem,
					registry: request.registry.to_string(),
					version: request.version.clone(),
					status: PackagePublishStatus::Blocked,
					message,
					placeholder: mode == PackagePublishRunMode::Placeholder,
					trusted_publishing: trust_handler
						.planned_trust_outcome(request, source, root, env_map),
					command: None,
					stdout: None,
					stderr: None,
					rate_limit: None,
					deferred_to: None,
				});

				continue;
			}

			primary_error = Some(MonochangeError::Config(message.clone()));
			append_publish_failure_outcomes(
				&mut outcomes,
				remaining_requests,
				mode,
				request,
				message,
				progress,
			);
			break;
		}

		let placeholder_dir = if mode == PackagePublishRunMode::Placeholder {
			match build_placeholder_directory(root, request, source, manifest_writers) {
				Ok(directory) => Some(directory),
				Err(error) => {
					let message = error.render();
					primary_error = Some(error);
					append_publish_failure_outcomes(
						&mut outcomes,
						remaining_requests,
						mode,
						request,
						message,
						progress,
					);
					break;
				}
			}
		} else {
			None
		};

		let mut publish_command = command_builder.build_publish_command(
			request,
			mode,
			placeholder_dir.as_ref().map(TempDir::path),
			dry_run,
		);
		publish_command.timeout = publish_command_timeout(request);

		if dry_run {
			progress.report(PublishProgressEvent::PackagePlanned(
				publish_progress_package(request),
			));

			if mode == PackagePublishRunMode::Placeholder {
				outcomes.push(PackagePublishOutcome {
					package: request.package_id.clone(),
					ecosystem: request.ecosystem,
					registry: request.registry.to_string(),
					version: request.version.clone(),
					status: PackagePublishStatus::Planned,
					message: planned_publish_message(mode, request),
					placeholder: true,
					trusted_publishing: trust_handler
						.planned_trust_outcome(request, source, root, env_map),
					command: None,
					stdout: None,
					stderr: None,
					rate_limit: None,
					deferred_to: None,
				});
				continue;
			}

			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				mode = ?mode,
				"validating package publish command (dry run)"
			);
		}

		if !dry_run && mode == PackagePublishRunMode::Release {
			let prerequisites = trust_handler
				.enforce_release_trust_prerequisites(request, source, root, env_map)
				.and_then(|()| {
					enforce_release_attestation_prerequisites(request, env_map, command_builder)
				});

			if let Err(error) = prerequisites {
				let message = error.render();
				primary_error = Some(error);
				append_publish_failure_outcomes(
					&mut outcomes,
					remaining_requests,
					mode,
					request,
					message,
					progress,
				);
				break;
			}
		}

		if !dry_run {
			progress.report(PublishProgressEvent::PackageStarted(
				publish_progress_package(request),
			));

			// A non-empty `PUB_TOKEN` publishes with that token; minting an OIDC
			// token would overwrite it.
			if request.registry == RegistryKind::PubDev
				&& publish_is_tag_bound(request, env_map)
				&& let Err(error) = refresh_pub_dev_trusted_publishing_token(
					client,
					env_map,
					executor,
					request,
					&mut publish_command,
				)
				.await
			{
				tracing::error!(
					package_name = request.package_name,
					version = %request.version,
					registry = %request.registry,
					error = %error,
					"failed to refresh the pub.dev trusted publishing token"
				);
				let message = error.render();
				primary_error = Some(error);
				append_publish_failure_outcomes(
					&mut outcomes,
					remaining_requests,
					mode,
					request,
					message,
					progress,
				);
				break;
			}
		}

		let output = match run_publish_command_with_retries(executor, &publish_command, request) {
			Ok(output) => output,
			Err(error) => {
				tracing::error!(
					package_name = request.package_name,
					version = %request.version,
					registry = %request.registry,
					error = %error,
					"publish command failed to execute"
				);
				let message = publish_command_failure_message(request, &error);
				append_publish_failure_outcomes(
					&mut outcomes,
					remaining_requests,
					mode,
					request,
					message,
					progress,
				);
				break;
			}
		};

		if !output.success {
			tracing::error!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				"publish command returned non-zero exit"
			);

			if dry_run {
				outcomes.push(PackagePublishOutcome {
					package: request.package_id.clone(),
					ecosystem: request.ecosystem,
					registry: request.registry.to_string(),
					version: request.version.clone(),
					status: PackagePublishStatus::Planned,
					message: planned_publish_message(mode, request),
					placeholder: mode == PackagePublishRunMode::Placeholder,
					trusted_publishing: trust_handler
						.planned_trust_outcome(request, source, root, env_map),
					command: Some(render_command(&publish_command)),
					stdout: non_empty_output(output.stdout),
					stderr: non_empty_output(output.stderr),
					rate_limit: None,
					deferred_to: None,
				});
				continue;
			}

			let mut outcome = failed_command_outcome(mode, request, &publish_command, &output);
			outcome.command = Some(render_command(&publish_command));
			outcome.stdout = non_empty_output(output.stdout);
			outcome.stderr = non_empty_output(output.stderr);
			progress.report(PublishProgressEvent::PackageFailed {
				package: publish_progress_package(request),
				message: outcome.message.clone(),
			});
			outcomes.push(outcome);
			append_skipped_after_failure_outcomes(
				&mut outcomes,
				remaining_requests,
				mode,
				request,
				progress,
			);
			break;
		}

		let trusted_publishing = if dry_run {
			trust_handler.planned_trust_outcome(request, source, root, env_map)
		} else if request.trusted_publishing.enabled {
			trust_handler.trust_outcome_for_skip(request, source, root, env_map)
		} else {
			disabled_trust_outcome()
		};

		let (status, message) = if dry_run {
			(
				PackagePublishStatus::Planned,
				planned_publish_message(mode, request),
			)
		} else if request.flow == PublishFlow::Staged {
			progress.report(PublishProgressEvent::PackageStaged(
				publish_progress_package(request),
			));
			(
				PackagePublishStatus::Staged,
				staged_publish_message(request),
			)
		} else {
			progress.report(PublishProgressEvent::PackagePublished(
				publish_progress_package(request),
			));
			(
				PackagePublishStatus::Published,
				format!(
					"published {} {} to {}",
					request.package_name, request.version, request.registry
				),
			)
		};

		info!(
			package_name = request.package_name,
			version = %request.version,
			registry = %request.registry,
			dry_run,
			"package publish command completed"
		);
		outcomes.push(PackagePublishOutcome {
			package: request.package_id.clone(),
			ecosystem: request.ecosystem,
			registry: request.registry.to_string(),
			version: request.version.clone(),
			status,
			message,
			placeholder: mode == PackagePublishRunMode::Placeholder,
			trusted_publishing,
			command: Some(render_command(&publish_command)),
			stdout: non_empty_output(output.stdout),
			stderr: non_empty_output(output.stderr),
			rate_limit: None,
			deferred_to: None,
		});
	}

	let report = PackagePublishReport {
		mode,
		dry_run,
		packages: outcomes,
	};
	let summary = report.summary();
	progress.report(PublishProgressEvent::RunFinished {
		mode,
		total: summary.total(),
		published: summary.published,
		staged: summary.staged,
		skipped: summary.planned
			+ summary.already_exists
			+ summary.blocked
			+ summary.not_attempted
			+ summary.deferred
			+ summary.out_of_scope,
		failed: summary.failed,
	});

	if let Some(error) = primary_error {
		return Err(PackagePublishFailure::new(error, report));
	}

	Ok(report)
}

pub fn build_placeholder_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let packages_by_config_id = packages_by_config_id(packages);
	let mut requests = Vec::new();

	for package_definition in &configuration.packages {
		let package = packages_by_config_id
			.get(package_definition.id.as_str())
			.copied();
		let should_publish =
			package.is_some_and(|package| package_can_be_published(package_definition, package));
		if let Some(package) = package.filter(|_| {
			should_publish
				&& (selected_packages.is_empty()
					|| selected_packages.contains(&package_definition.id))
		}) {
			requests.push(PublishRequest {
				package_id: package_definition.id.clone(),
				package_name: package.name.clone(),
				ecosystem: package.ecosystem,
				manifest_path: package.manifest_path.clone(),
				package_root: package
					.manifest_path
					.parent()
					.unwrap_or(&package.workspace_root)
					.to_path_buf(),
				registry: resolve_registry_kind(
					package_definition.publish.registry.as_ref(),
					package.ecosystem,
				)?,
				package_manager: package.metadata.get("manager").cloned(),
				package_metadata: package.metadata.clone(),
				mode: package_definition.publish.mode,
				// Placeholders register the package immediately; staged
				// publishing would leave the name unclaimed until approval.
				flow: PublishFlow::Direct,
				version: PLACEHOLDER_VERSION.to_string(),
				placeholder: true,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
				attestations: package_definition.publish.attestations.clone(),
				timeout: package_definition.publish.timeout.clone(),
				fail_on_duplicate: package_definition.publish.fail_on_duplicate,
				placeholder_readme: resolve_placeholder_readme(
					root,
					package_definition.publish.placeholder.readme.as_deref(),
					package_definition
						.publish
						.placeholder
						.readme_file
						.as_deref(),
					&package.name,
				)?,
				release_tag: None,
				run_scope: PublishRunScope::Publish,
			});
		}
	}

	requests.sort_by(|left, right| left.package_id.cmp(&right.package_id));
	Ok(requests)
}

pub fn configured_package_publication_targets(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> Vec<PackagePublicationTarget> {
	let packages_by_config_id = packages_by_config_id(packages);
	configuration
		.packages
		.iter()
		.filter_map(|package_definition| {
			let package = packages_by_config_id
				.get(package_definition.id.as_str())
				.copied()?;
			let version = package.current_version.as_ref()?.to_string();
			Some(PackagePublicationTarget {
				package: package_definition.id.clone(),
				ecosystem: package.ecosystem,
				registry: package_definition.publish.registry.clone(),
				version,
				mode: package_definition.publish.mode,
				flow: package_definition.publish.flow,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
				attestations: package_definition.publish.attestations.clone(),
				timeout: package_definition.publish.timeout.clone(),
				fail_on_duplicate: package_definition.publish.fail_on_duplicate,
			})
		})
		.collect()
}

pub fn build_configured_package_release_requests(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let publications = configured_package_publication_targets(configuration, packages);
	build_release_requests(configuration, packages, &publications, selected_packages)
}

pub async fn build_pending_configured_package_release_requests(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let requests =
		build_configured_package_release_requests(configuration, packages, selected_packages)?;
	filter_pending_publish_requests(&requests).await
}

pub fn build_release_requests(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	publications: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let packages_by_config_id = packages_by_config_id(packages);
	let mut requests = Vec::new();

	for publication in publications {
		if !selected_packages.is_empty() && !selected_packages.contains(&publication.package) {
			continue;
		}

		let Some(package_definition) = configuration.package_by_id(&publication.package) else {
			continue;
		};
		let Some(package) = packages_by_config_id
			.get(publication.package.as_str())
			.copied()
		else {
			continue;
		};

		if !package_can_be_published(package_definition, package) {
			continue;
		}

		requests.push(PublishRequest {
			package_id: publication.package.clone(),
			package_name: package.name.clone(),
			ecosystem: package.ecosystem,
			manifest_path: package.manifest_path.clone(),
			package_root: package
				.manifest_path
				.parent()
				.unwrap_or(&package.workspace_root)
				.to_path_buf(),
			registry: resolve_registry_kind(publication.registry.as_ref(), package.ecosystem)?,
			package_manager: package.metadata.get("manager").cloned(),
			package_metadata: package.metadata.clone(),
			mode: publication.mode,
			flow: publication.flow,
			version: publication.version.clone(),
			placeholder: false,
			trusted_publishing: publication.trusted_publishing.clone(),
			attestations: publication.attestations.clone(),
			timeout: publication.timeout.clone(),
			fail_on_duplicate: publication.fail_on_duplicate,
			placeholder_readme: default_placeholder_readme(&package.name),
			release_tag: package_release_tag(
				configuration,
				&publication.package,
				&publication.version,
			)?,
			run_scope: PublishRunScope::Publish,
		});
	}

	order_release_requests_by_publish_dependencies(configuration, packages, requests)
}

/// Render the release tag monochange creates for `package_id` at `version`.
///
/// The tag belongs to the package's release owner: its group when it has
/// one, otherwise the package. Returns `None` when that owner sets
/// `tag = false`.
fn package_release_tag(
	configuration: &WorkspaceConfiguration,
	package_id: &str,
	version: &str,
) -> MonochangeResult<Option<PublishReleaseTag>> {
	if !configuration
		.effective_release_identity(package_id)
		.is_some_and(|identity| identity.tag)
	{
		return Ok(None);
	}

	Ok(
		configured_release_tag_name(configuration, package_id, version)?
			.map(|name| PublishReleaseTag { name }),
	)
}

/// The tags a release of every configured package and group creates,
/// rendered from configuration and discovered package versions.
///
/// Use this when no release record lists the release's tags, for example
/// when publishing every configured package (`--all`). Every configured
/// group and standalone package with `tag = true` and a discovered current
/// version contributes its tag—including non-publishable packages and groups
/// whose members are all non-publishable—so scoping sees every tag the
/// release creates.
pub fn configured_release_tags(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<PublishReleaseTags> {
	let packages_by_config_id = packages_by_config_id(packages);
	let mut release_tags = PublishReleaseTags::default();

	for group in &configuration.groups {
		if !group.tag {
			continue;
		}

		let Some(version) = group
			.packages
			.iter()
			.filter_map(|member| packages_by_config_id.get(member.as_str()).copied())
			.filter_map(|package| package.current_version.as_ref())
			.max()
		else {
			continue;
		};

		let tag_name = group
			.version_format
			.render_tag(&group.id, &version.to_string(), "group")?;

		if group.version_format.is_primary() {
			release_tags.primary = Some(tag_name.clone());
		}

		release_tags.tags.insert(tag_name);
	}

	for package_definition in &configuration.packages {
		if !package_definition.tag
			|| configuration
				.group_for_package(&package_definition.id)
				.is_some()
		{
			continue;
		}

		let Some(version) = packages_by_config_id
			.get(package_definition.id.as_str())
			.and_then(|package| package.current_version.as_ref())
		else {
			continue;
		};

		let tag_name = package_definition.version_format.render_tag(
			&package_definition.id,
			&version.to_string(),
			package_definition.package_type.as_str(),
		)?;

		if package_definition.version_format.is_primary() {
			release_tags.primary = Some(tag_name.clone());
		}

		release_tags.tags.insert(tag_name);
	}

	Ok(release_tags)
}

/// The tag name a release owner renders for `package_id` at `version`,
/// whether or not the owner creates tags.
fn configured_release_tag_name(
	configuration: &WorkspaceConfiguration,
	package_id: &str,
	version: &str,
) -> MonochangeResult<Option<String>> {
	let Some(package_definition) = configuration.package_by_id(package_id) else {
		return Ok(None);
	};

	if let Some(group) = configuration.group_for_package(package_id) {
		return group
			.version_format
			.render_tag(&group.id, version, "group")
			.map(Some);
	}

	package_definition
		.version_format
		.render_tag(
			&package_definition.id,
			version,
			package_definition.package_type.as_str(),
		)
		.map(Some)
}

pub fn resolve_registry_kind(
	registry: Option<&PublishRegistry>,
	ecosystem: Ecosystem,
) -> MonochangeResult<RegistryKind> {
	match registry {
		Some(PublishRegistry::Builtin(registry)) => Ok(*registry),
		Some(PublishRegistry::Custom(name)) => {
			Err(MonochangeError::Config(format!(
				"built-in package publishing does not support custom registry `{name}`"
			)))
		}
		None => default_registry_kind_for_ecosystem(ecosystem.as_str()),
	}
}

pub fn default_registry_kind_for_ecosystem(ecosystem: &str) -> MonochangeResult<RegistryKind> {
	let parsed = ecosystem.parse::<Ecosystem>().map_err(|()| {
		MonochangeError::Config(format!(
			"built-in package publishing does not support ecosystem `{ecosystem}`"
		))
	})?;
	Ok(monochange_core::default_registry_kind_for_ecosystem(parsed)
		.expect("all built-in ecosystems have default registries"))
}

pub fn resolve_placeholder_readme(
	root: &Path,
	inline: Option<&str>,
	file: Option<&Path>,
	package_name: &str,
) -> MonochangeResult<String> {
	if let Some(inline) = inline {
		return Ok(inline.to_string());
	}

	if let Some(file) = file {
		let path = root.join(file);
		return fs::read_to_string(&path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read placeholder README {}: {error}",
				path.display()
			))
		});
	}

	Ok(default_placeholder_readme(package_name))
}

pub fn default_placeholder_readme(package_name: &str) -> String {
	format!(
		"# {package_name}\n\nThis is a placeholder release published by monochange to bootstrap trusted publishing.\n"
	)
}

/// Adapter for building ecosystem-specific publish commands.
pub trait PublishAdapter {
	fn registry_kind(&self) -> RegistryKind;
	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec>;
	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec>;
	fn append_dry_run_args(&self, args: &mut Vec<String>) {
		args.push("--dry-run".to_string());
	}
	fn supported_providers(&self) -> Vec<CiProviderKind> {
		Vec::new()
	}
	fn registry_setup_url(&self) -> Option<&'static str> {
		None
	}
	fn registry_notes(&self) -> Vec<String> {
		vec!["unknown registry capabilities are treated as unsupported".to_string()]
	}
	fn supports_provenance(&self) -> bool {
		false
	}
}

/// Registry of publish adapters used to dispatch publish command construction.
#[derive(Default)]
pub struct PublishCommandBuilder {
	adapters: Vec<Box<dyn PublishAdapter>>,
}

impl PublishCommandBuilder {
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	#[must_use]
	pub fn with_adapter(mut self, adapter: Box<dyn PublishAdapter>) -> Self {
		self.adapters.push(adapter);
		self
	}

	pub fn push_adapter(&mut self, adapter: Box<dyn PublishAdapter>) {
		self.adapters.push(adapter);
	}

	pub fn adapter_for_registry(&self, registry: RegistryKind) -> Option<&dyn PublishAdapter> {
		self.adapters
			.iter()
			.find(|adapter| adapter.registry_kind() == registry)
			.map(AsRef::as_ref)
	}

	pub fn build_publish_command(
		&self,
		request: &PublishRequest,
		mode: PackagePublishRunMode,
		placeholder_dir: Option<&Path>,
		dry_run: bool,
	) -> CommandSpec {
		let adapter = self
			.adapter_for_registry(request.registry)
			.expect("unsupported built-in publish registry");

		let mut command = match mode {
			PackagePublishRunMode::Placeholder => {
				let path = placeholder_dir.expect("placeholder directory must exist");
				adapter.build_placeholder_command(request, path)
			}
			PackagePublishRunMode::Release => adapter.build_release_command(request),
		}
		.expect("unsupported publish mode for this registry");

		if dry_run {
			adapter.append_dry_run_args(&mut command.args);
		}

		if request
			.package_metadata
			.get(PUBLISH_STREAM_OUTPUT_METADATA_KEY)
			.is_some_and(|value| value == "true")
		{
			command.env.insert(
				PUBLISH_STREAM_OUTPUT_ENV_KEY.to_string(),
				"true".to_string(),
			);
		}
		command
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandOutput {
	pub success: bool,
	pub stdout: String,
	pub stderr: String,
}

pub trait CommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput>;
}

pub struct ProcessCommandExecutor {
	stream_output: bool,
}

impl ProcessCommandExecutor {
	#[must_use]
	pub fn new(stream_output: bool) -> Self {
		Self { stream_output }
	}
}

impl CommandExecutor for ProcessCommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		let stream_output = self.stream_output
			|| spec
				.env
				.get(PUBLISH_STREAM_OUTPUT_ENV_KEY)
				.is_some_and(|value| value == "true");

		if stream_output {
			return run_streaming_process_command(spec);
		}

		run_captured_process_command(spec)
	}
}

fn run_captured_process_command(spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
	use std::process::Command;

	let mut command = Command::new(&spec.program);
	command
		.args(&spec.args)
		.current_dir(&spec.cwd)
		.envs(&spec.env)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child = command.spawn().map_err(|error| {
		MonochangeError::Io(format!(
			"failed to run `{}` in {}: {error}",
			render_command(spec),
			spec.cwd.display()
		))
	})?;
	let stdout = child
		.stdout
		.take()
		.expect("stdout was configured for piping");
	let stderr = child
		.stderr
		.take()
		.expect("stderr was configured for piping");
	let stdout_thread = std::thread::spawn(move || read_child_output(stdout));
	let stderr_thread = std::thread::spawn(move || read_child_output(stderr));
	let status = wait_child_with_timeout(&mut child, spec)?;

	let stdout = join_child_output(stdout_thread, spec, "stdout")?;
	let stderr = join_child_output(stderr_thread, spec, "stderr")?;

	Ok(CommandOutput {
		success: status.success(),
		stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
		stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
	})
}

fn run_streaming_process_command(spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
	use std::process::Command;

	let mut command = Command::new(&spec.program);
	command
		.args(&spec.args)
		.current_dir(&spec.cwd)
		.envs(&spec.env)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child = command
		.spawn()
		.map_err(|error| process_command_error(spec, "run", &error))?;

	let stdout = child
		.stdout
		.take()
		.expect("stdout was configured for piping");
	let stderr = child
		.stderr
		.take()
		.expect("stderr was configured for piping");
	let stdout_thread = std::thread::spawn(move || tee_child_output(stdout, std::io::stdout()));
	let stderr_thread = std::thread::spawn(move || tee_child_output(stderr, std::io::stderr()));
	let status = wait_child_with_timeout(&mut child, spec)?;
	let stdout = join_child_output(stdout_thread, spec, "stdout")?;
	let stderr = join_child_output(stderr_thread, spec, "stderr")?;

	Ok(CommandOutput {
		success: status.success(),
		stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
		stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
	})
}

fn wait_child_with_timeout(
	child: &mut std::process::Child,
	spec: &CommandSpec,
) -> MonochangeResult<std::process::ExitStatus> {
	let Some(timeout) = spec.timeout else {
		return child
			.wait()
			.map_err(|error| process_command_error(spec, "wait for", &error));
	};
	let deadline = std::time::Instant::now() + timeout;

	loop {
		match child.try_wait() {
			Ok(Some(status)) => return Ok(status),
			Ok(None) => {
				if std::time::Instant::now() >= deadline {
					let _ = child.kill();
					let _ = child.wait();
					return Err(publish_timeout_error(spec, timeout));
				}

				std::thread::sleep(Duration::from_millis(100));
			}
			// patch-coverage:ignore-start -- `try_wait` errors require an invalid child handle that cannot be produced by a successful spawn
			Err(error) => return Err(process_command_error(spec, "wait for", &error)),
			// patch-coverage:ignore-end
		}
	}
}

fn read_child_output(mut reader: impl std::io::Read) -> std::io::Result<Vec<u8>> {
	let mut buffer = Vec::new();
	std::io::Read::read_to_end(&mut reader, &mut buffer)?;
	Ok(buffer)
}

fn publish_timeout_error(spec: &CommandSpec, timeout: Duration) -> MonochangeError {
	MonochangeError::Io(format!(
		"{PUBLISH_TIMEOUT_ERROR_PREFIX}: `{}` in {} timed out after {} seconds",
		render_command(spec),
		spec.cwd.display(),
		timeout.as_secs()
	))
}

fn publish_command_timeout(request: &PublishRequest) -> Option<Duration> {
	if request.timeout.disabled() {
		None
	} else {
		Some(Duration::from_secs(request.timeout.timeout_seconds))
	}
}

/// Returns `true` when a [`CommandExecutor::run`] failure was caused by the
/// configured publish timeout rather than a registry or command error.
#[must_use]
pub fn is_publish_timeout_error(error: &MonochangeError) -> bool {
	error.render().contains(PUBLISH_TIMEOUT_ERROR_PREFIX)
}

fn run_publish_command_with_retries(
	executor: &mut dyn CommandExecutor,
	publish_command: &CommandSpec,
	request: &PublishRequest,
) -> Result<CommandOutput, MonochangeError> {
	let max_attempts = 1usize + request.timeout.retries as usize;
	let mut last_error = None;

	for attempt in 1..=max_attempts {
		match executor.run(publish_command) {
			Ok(output) => return Ok(output),
			Err(error) => {
				let timed_out = is_publish_timeout_error(&error);
				tracing::warn!(
					package_name = request.package_name,
					version = %request.version,
					registry = %request.registry,
					attempt,
					max_attempts,
					timed_out,
					error = %error,
					"publish command attempt failed"
				);
				last_error = Some(error);
				if !timed_out || attempt == max_attempts {
					break;
				}
			}
		}
	}

	Err(last_error.expect("retry loop runs at least once"))
}

fn publish_command_failure_message(request: &PublishRequest, error: &MonochangeError) -> String {
	if is_publish_timeout_error(error) {
		let mut message = format!(
			"publish for `{}` version `{}` timed out after {} attempt(s); {}",
			request.package_name,
			request.version,
			request.timeout.retries + 1,
			error.render()
		);

		if request.registry == RegistryKind::PubDev {
			message.push_str("\n\n");
			message.push_str(dart_protected_publishing_guidance());
		}

		return message;
	}

	error.render()
}

/// Explanatory guidance for Dart/pub.dev trusted-publishing failures and
/// timeouts describing pub.dev's run-ref requirement.
pub fn dart_protected_publishing_guidance() -> &'static str {
	"pub.dev trusted publishing only accepts runs whose OIDC token ref is `refs/tags/<tag-pattern>`, where `<tag-pattern>` is configured in the package's pub.dev admin and contains `{{version}}` matching the published version. The `Enable publishing from workflow_dispatch events` option on pub.dev only allows the event name; the run must still be dispatched on the tag ref (for example `gh workflow run <workflow>.yml --ref <tag>`), or the publish must run from the tag-push event. As a last resort, register a non-OIDC credential with `dart pub token add https://pub.dev --env-var PUB_TOKEN` before publishing."
}

/// Whether `registry` accepts a trusted upload only from a CI run on the
/// package's release tag.
///
/// pub.dev compares the GitHub Actions OIDC token's `ref` claim with
/// `refs/tags/<tag-pattern>`, where the package's pub.dev admin page sets the
/// tag pattern and `{{version}}` becomes the version being published. The
/// rule applies to `push` and `workflow_dispatch` runs alike, so a run on any
/// other tag or on a branch is always rejected.
#[must_use]
pub fn registry_binds_trusted_publish_to_tag(
	registry: RegistryKind,
	trusted_publishing: &TrustedPublishingSettings,
) -> bool {
	registry == RegistryKind::PubDev && trusted_publishing.enabled
}

/// Whether publishing `request` in this environment can only succeed from a
/// run on the package's release tag.
///
/// Packages published externally (`publish.mode = "external"`) never upload
/// through monochange, and a `PUB_TOKEN` credential publishes without
/// trusted publishing, so the tag rule applies to neither. An empty
/// `PUB_TOKEN`, which GitHub Actions passes for an unset secret, is no
/// credential.
#[must_use]
pub fn publish_is_tag_bound(request: &PublishRequest, env_map: &BTreeMap<String, String>) -> bool {
	request.mode == PublishMode::Builtin
		&& registry_binds_trusted_publish_to_tag(request.registry, &request.trusted_publishing)
		&& !has_pub_token(env_map)
}

fn has_pub_token(env_map: &BTreeMap<String, String>) -> bool {
	env_map
		.get(PUB_TOKEN_ENV_VAR)
		.is_some_and(|token| !token.trim().is_empty())
}

/// Whether the environment is a GitHub Actions run, by the same test
/// [`detect_trusted_publishing_identity`] uses.
fn is_github_actions(env_map: &BTreeMap<String, String>) -> bool {
	env_is_true(env_map, "GITHUB_ACTIONS") || env_map.contains_key("GITHUB_WORKFLOW_REF")
}

fn non_empty_env<'a>(env_map: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
	env_map
		.get(name)
		.map(String::as_str)
		.filter(|value| !value.is_empty())
}

/// The git ref (`GITHUB_REF`) of the current GitHub Actions run, or `None`
/// outside GitHub Actions or when the run has no ref.
#[must_use]
pub fn github_actions_run_ref(env_map: &BTreeMap<String, String>) -> Option<&str> {
	if !is_github_actions(env_map) {
		return None;
	}

	non_empty_env(env_map, "GITHUB_REF")
}

/// The events pub.dev accepts a trusted upload from.
const PUB_DEV_TRUSTED_PUBLISH_EVENTS: [&str; 2] = ["push", "workflow_dispatch"];

/// What pub.dev checks about the current GitHub Actions run.
#[derive(Debug, Clone, Copy)]
struct GitHubActionsRun<'a> {
	git_ref: Option<&'a str>,
	ref_type: Option<&'a str>,
	event: Option<&'a str>,
}

impl<'a> GitHubActionsRun<'a> {
	fn detect(env_map: &'a BTreeMap<String, String>) -> Option<Self> {
		is_github_actions(env_map).then(|| {
			Self {
				git_ref: github_actions_run_ref(env_map),
				ref_type: non_empty_env(env_map, "GITHUB_REF_TYPE"),
				event: non_empty_env(env_map, "GITHUB_EVENT_NAME"),
			}
		})
	}

	/// The tag the run is on, when its ref is a tag.
	fn tag(self) -> Option<&'a str> {
		if self.ref_type.is_some_and(|ref_type| ref_type != "tag") {
			return None;
		}

		self.git_ref?.strip_prefix("refs/tags/")
	}

	/// The run's event when pub.dev rejects trusted uploads from it. An
	/// unknown event is not known to be rejected.
	fn rejected_event(self) -> Option<&'a str> {
		self.event
			.filter(|event| !PUB_DEV_TRUSTED_PUBLISH_EVENTS.contains(event))
	}
}

/// How the current publish run treats one package.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PublishRunScope {
	/// Publish (or plan) the package in this run.
	Publish,
	/// The registry would reject the package's upload from this run.
	/// `deferral` names why and the run that publishes it; `message`
	/// explains both.
	DeferredToTag {
		deferral: PublishDeferral,
		message: String,
	},
	/// Another run of the release publishes the package: the run on its home
	/// tag (see [`plan_publish_run_scopes`]). `message` names that run.
	OutOfScope { message: String },
	/// The registry accepts the package only from a run on its release tag,
	/// but its release owner creates no tag, so no run can publish it.
	MissingReleaseTag,
}

/// Decide, for every request of the current run, whether the run publishes
/// it, and store the decision in [`PublishRequest::run_scope`].
///
/// The decision depends on every package of the release, so
/// `release_requests` holds the whole release, before any `--package`,
/// `--group`, or `--ecosystem` selection; `requests` are the selected
/// ones. A request that is not part of the release keeps its scope.
pub fn assign_publish_run_scopes(
	requests: &mut [PublishRequest],
	release_requests: &[PublishRequest],
	release_tags: &PublishReleaseTags,
	env_map: &BTreeMap<String, String>,
) {
	let scopes = release_requests
		.iter()
		.map(publish_request_key)
		.zip(plan_publish_run_scopes(
			release_requests,
			release_tags,
			env_map,
		))
		.collect::<BTreeMap<_, _>>();

	for request in requests {
		if let Some(scope) = scopes.get(&publish_request_key(request)) {
			request.run_scope = scope.clone();
		}
	}
}

/// What identifies one publication of a release: the package, its registry,
/// and its version.
fn publish_request_key(request: &PublishRequest) -> (String, String, String) {
	(
		request.package_id.clone(),
		request.registry.to_string(),
		request.version.clone(),
	)
}

/// Decide, for every request of a release, whether the current run
/// publishes it.
///
/// A request is *tag-bound* when [`publish_is_tag_bound`] holds: pub.dev
/// accepts it only from a `push` or `workflow_dispatch` run on its release
/// tag. Outside GitHub Actions the run's ref is unknown, and every request
/// publishes; so does every request of a release without tag-bound
/// requests. Otherwise every request has one *home*: the release tag whose
/// run publishes it (see [`ReleaseTagRuns`]).
///
/// - **A run on one of the release's tags** publishes only the requests whose
///   home is that tag. A tag-bound request with another home is
///   [`DeferredToTag`](PublishRunScope::DeferredToTag) in the release's
///   *coordinator* run (the primary tag's run, or without a primary tag the
///   first ordinary tag's run), so exactly one run dispatches it, and
///   [`OutOfScope`](PublishRunScope::OutOfScope) in every other run. Every
///   other request with another home is out of scope, and its message names
///   its home run.
/// - **A run on any other ref** (a branch, or a tag the release does not
///   create) publishes every request that is not tag-bound and defers the
///   tag-bound ones.
///
/// A tag-bound request on its own tag is still deferred when the run's event
/// is not `push` or `workflow_dispatch`, and a tag-bound request without a
/// release tag is [`MissingReleaseTag`](PublishRunScope::MissingReleaseTag).
#[must_use]
pub fn plan_publish_run_scopes(
	requests: &[PublishRequest],
	release_tags: &PublishReleaseTags,
	env_map: &BTreeMap<String, String>,
) -> Vec<PublishRunScope> {
	let Some(run) = GitHubActionsRun::detect(env_map) else {
		return vec![PublishRunScope::Publish; requests.len()];
	};
	let tag_bound = requests
		.iter()
		.map(|request| publish_is_tag_bound(request, env_map))
		.collect::<Vec<_>>();

	if !tag_bound.contains(&true) {
		return vec![PublishRunScope::Publish; requests.len()];
	}

	let tag_runs = ReleaseTagRuns::new(requests, &tag_bound, release_tags);
	let release_tag_run = run.tag().filter(|tag| tag_runs.tags.contains(*tag));

	requests
		.iter()
		.zip(&tag_bound)
		.map(|(request, &bound)| {
			if !bound {
				return match (release_tag_run, tag_runs.home_of_untethered(request)) {
					(Some(run_tag), Some(home)) if run_tag != home => {
						PublishRunScope::OutOfScope {
							message: out_of_scope_message(
								request,
								home,
								", because a release with trusted pub.dev packages publishes each package from one of its tags",
							),
						}
					}
					_ => PublishRunScope::Publish,
				};
			}

			let Some(tag) = request.release_tag.as_ref() else {
				return PublishRunScope::MissingReleaseTag;
			};

			match release_tag_run {
				Some(run_tag)
					if run_tag != tag.name && Some(run_tag) != tag_runs.coordinator.as_deref() =>
				{
					PublishRunScope::OutOfScope {
						message: tag_bound_out_of_scope_message(request, tag, &tag_runs, env_map),
					}
				}
				_ => tag_bound_run_scope(request, tag, run, env_map),
			}
		})
		.collect()
}

/// The release tags whose runs publish a release with tag-bound requests,
/// and which of them publishes each request.
///
/// A tag-bound request's home is its own release tag. Tags of tag-bound
/// requests other than the primary tag are *bound* tags; every other release
/// tag is *ordinary*. Every other request's home is the primary tag when the
/// release has one; otherwise its own tag when that tag is ordinary, then
/// the first ordinary tag, then its own (bound) tag, then the first bound
/// tag.
#[derive(Debug)]
struct ReleaseTagRuns {
	/// Every tag of the release.
	tags: BTreeSet<String>,
	primary: Option<String>,
	ordinary: BTreeSet<String>,
	bound: BTreeSet<String>,
	/// The run that defers tag-bound requests to their tags: the primary
	/// tag's run, or the first ordinary tag's run.
	coordinator: Option<String>,
}

impl ReleaseTagRuns {
	fn new(
		requests: &[PublishRequest],
		tag_bound: &[bool],
		release_tags: &PublishReleaseTags,
	) -> Self {
		let primary = release_tags.primary.clone();
		let release_tag_of =
			|request: &PublishRequest| request.release_tag.as_ref().map(|tag| tag.name.clone());
		let tags = release_tags
			.tags
			.iter()
			.cloned()
			.chain(primary.clone())
			.chain(requests.iter().filter_map(release_tag_of))
			.collect::<BTreeSet<_>>();
		let bound = requests
			.iter()
			.zip(tag_bound)
			.filter(|(_, bound)| **bound)
			.filter_map(|(request, _)| release_tag_of(request))
			.filter(|tag| Some(tag) != primary.as_ref())
			.collect::<BTreeSet<_>>();
		let ordinary = tags.difference(&bound).cloned().collect::<BTreeSet<_>>();
		let coordinator = primary.clone().or_else(|| ordinary.first().cloned());

		Self {
			tags,
			primary,
			ordinary,
			bound,
			coordinator,
		}
	}

	/// The home of a request that is not tag-bound, or `None` when the
	/// release has no tags.
	fn home_of_untethered<'a>(&'a self, request: &'a PublishRequest) -> Option<&'a str> {
		let own = request.release_tag.as_ref().map(|tag| tag.name.as_str());

		self.primary
			.as_deref()
			.or_else(|| own.filter(|tag| self.ordinary.contains(*tag)))
			.or(self.coordinator.as_deref())
			.or(own)
			.or_else(|| self.bound.first().map(String::as_str))
	}
}

/// The scope of a tag-bound request with release tag `tag` in `run`.
fn tag_bound_run_scope(
	request: &PublishRequest,
	tag: &PublishReleaseTag,
	run: GitHubActionsRun<'_>,
	env_map: &BTreeMap<String, String>,
) -> PublishRunScope {
	let reason = match (run.git_ref, run.tag()) {
		(None, _) => PublishDeferralReason::UnknownRef,
		(Some(_), None) => PublishDeferralReason::WrongRefType,
		(Some(_), Some(run_tag)) if run_tag != tag.name => PublishDeferralReason::WrongRef,
		(Some(_), Some(_)) => {
			if run.rejected_event().is_none() {
				return PublishRunScope::Publish;
			}

			PublishDeferralReason::WrongEvent
		}
	};
	let deferral = publish_deferral(request, tag, reason, env_map);
	let message = deferral_message(request, &deferral, run);

	PublishRunScope::DeferredToTag { deferral, message }
}

/// Quote one POSIX shell argument only when it contains characters outside
/// a safe unquoted set, keeping clean identifiers readable.
fn shell_quote_arg(value: &str) -> String {
	if !value.is_empty()
		&& value.bytes().all(|byte| {
			byte.is_ascii_alphanumeric()
				|| matches!(byte, b'_' | b'.' | b'/' | b':' | b'@' | b'+' | b'-')
		}) {
		return value.to_string();
	}

	format!("'{}'", value.replace('\'', "'\\''"))
}

/// Describe the run that publishes `request` from `tag`: the package's
/// publish workflow dispatched on the tag.
///
/// The workflow is `publish.trusted_publishing.workflow` when the package
/// sets one, because pub.dev trusts that workflow; otherwise it is the
/// current run's workflow. When no workflow can be determined, `command`
/// is `None` rather than emitting an unrunnable placeholder.
fn publish_deferral(
	request: &PublishRequest,
	tag: &PublishReleaseTag,
	reason: PublishDeferralReason,
	env_map: &BTreeMap<String, String>,
) -> PublishDeferral {
	let workflow = request
		.trusted_publishing
		.workflow
		.as_deref()
		.map(str::trim)
		.filter(|workflow| !workflow.is_empty())
		.map(str::to_string)
		.or_else(|| {
			env_map
				.get("GITHUB_WORKFLOW_REF")
				.and_then(|value| parse_github_workflow_ref(value))
		})
		.or_else(|| non_empty_env(env_map, "GITHUB_WORKFLOW").map(str::to_string));
	let repository = request
		.trusted_publishing
		.repository
		.as_deref()
		.map(str::trim)
		.filter(|repository| !repository.is_empty())
		.or_else(|| non_empty_env(env_map, "GITHUB_REPOSITORY"))
		.map(|repository| format!(" --repo {}", shell_quote_arg(repository)))
		.unwrap_or_default();
	let command = workflow.map(|workflow| {
		format!(
			"gh workflow run {}{repository} --ref {}",
			shell_quote_arg(&workflow),
			shell_quote_arg(&tag.name)
		)
	});

	PublishDeferral {
		reason,
		tag: tag.name.clone(),
		git_ref: tag.git_ref(),
		command,
	}
}

/// Explain why `request` is deferred and what publishes it.
fn deferral_message(
	request: &PublishRequest,
	deferral: &PublishDeferral,
	run: GitHubActionsRun<'_>,
) -> String {
	let registry = registry_display_name(request.registry.as_str());
	let package = format!("{} {}", request.package_name, request.version);
	let required_ref = &deferral.git_ref;
	let run_ref = run.git_ref.unwrap_or_default();
	let unknown_workflow_hint = format!(
		"dispatch its publish workflow on `{}` (set `publish.trusted_publishing.workflow` or keep `GITHUB_WORKFLOW_REF` so monochange can render the dispatch command)",
		deferral.tag
	);

	match deferral.reason {
		PublishDeferralReason::WrongRef => {
			let action = deferral.command.as_deref().map_or_else(
				|| unknown_workflow_hint.clone(),
				|command| format!("publish it with `{command}`"),
			);
			format!(
				"{registry} only accepts {package} from a run on `{required_ref}`, and this run is on `{run_ref}`; {action}"
			)
		}
		PublishDeferralReason::WrongRefType => {
			let ref_type = run.ref_type.unwrap_or("ref");
			let action = deferral.command.as_deref().map_or_else(
				|| unknown_workflow_hint.clone(),
				|command| format!("publish it with `{command}`"),
			);
			format!(
				"{registry} only accepts {package} from a run on the tag `{required_ref}`, and this run is on the {ref_type} `{run_ref}`; {action}"
			)
		}
		PublishDeferralReason::WrongEvent => {
			let event = run.event.unwrap_or_default();
			let now = deferral.command.as_deref().map_or_else(
				|| format!("To publish this version now, {unknown_workflow_hint}"),
				|command| format!("To publish this version now, run `{command}`"),
			);
			format!(
				"{registry} only accepts {package} from a run that a `push` or `workflow_dispatch` event starts, and a `{event}` event started this run on `{required_ref}`. No `{event}` run ever publishes it, so start the publish workflow from tag pushes (`on: push`) instead of `on: {event}`. {now}"
			)
		}
		PublishDeferralReason::UnknownRef => {
			format!(
				"{registry} only accepts {package} from a run on `{required_ref}`, and this GitHub Actions run has no `GITHUB_REF`, so monochange cannot tell which ref it is on. No run publishes it until the publish step's environment keeps `GITHUB_REF`: every run without it defers it again"
			)
		}
	}
}

/// Explain that `request` publishes in the run on its home tag `home`.
fn out_of_scope_message(request: &PublishRequest, home: &str, detail: &str) -> String {
	format!(
		"not published in this run: {} {} publishes in the run on `refs/tags/{home}`{detail}",
		request.package_name, request.version
	)
}

/// Explain that tag-bound `request` publishes in the run on its tag, and
/// what starts that run.
fn tag_bound_out_of_scope_message(
	request: &PublishRequest,
	tag: &PublishReleaseTag,
	tag_runs: &ReleaseTagRuns,
	env_map: &BTreeMap<String, String>,
) -> String {
	let command = publish_deferral(request, tag, PublishDeferralReason::WrongRef, env_map).command;
	let start = match (tag_runs.coordinator.as_deref(), command.as_deref()) {
		(Some(coordinator), _) if coordinator == tag.name => String::new(),
		(Some(coordinator), Some(command)) => {
			format!("; the run on `refs/tags/{coordinator}` dispatches it with `{command}`")
		}
		(Some(coordinator), None) => {
			format!(
				"; the run on `refs/tags/{coordinator}` defers it to `{}` (set `publish.trusted_publishing.workflow` or keep `GITHUB_WORKFLOW_REF` so monochange can render the dispatch command)",
				tag.name
			)
		}
		(None, Some(command)) => format!("; start it with `{command}`"),
		(None, None) => {
			format!(
				"; dispatch its publish workflow on `{}` (set `publish.trusted_publishing.workflow` or keep `GITHUB_WORKFLOW_REF` so monochange can render the dispatch command)",
				tag.name
			)
		}
	};

	out_of_scope_message(
		request,
		&tag.name,
		&format!(", the only run pub.dev accepts it from{start}"),
	)
}

/// Message for a tag-bound package whose release owner creates no tag.
///
/// `owner_section` names the section that owns the package's release tag
/// (see [`release_tag_owner_section`]); `None` names the package and its
/// group generically.
#[must_use]
pub fn missing_release_tag_message(package_id: &str, owner_section: Option<&str>) -> String {
	let owner = owner_section.map_or_else(
		|| format!("`[package.{package_id}]` (or on its group)"),
		|section| format!("`{section}`"),
	);
	format!(
		"`{package_id}` publishes to pub.dev with trusted publishing, which only accepts an upload from a run on the package's release tag (`refs/tags/<tag>`), but monochange creates no tag for it. Set `tag = true` on {owner} so every release creates the tag, or set `publish.trusted_publishing = false` on `[package.{package_id}]` to publish with a pub.dev token instead."
	)
}

/// The configuration section that owns a package's release tag:
/// `[group.<id>]` for a group member, otherwise `[package.<id>]`.
#[must_use]
pub fn release_tag_owner_section(
	configuration: &WorkspaceConfiguration,
	package_id: &str,
) -> String {
	configuration.group_for_package(package_id).map_or_else(
		|| format!("[package.{package_id}]"),
		|group| format!("[group.{}]", group.id),
	)
}

/// Report every package that publishes to a tag-bound registry with the
/// built-in publisher but gets no release tag, so it can never publish.
///
/// Packages that never publish, such as a Dart package with
/// `publish_to: none` (see [`package_can_be_published`]), need no tag.
/// `discover_packages` reads the workspace's packages; it runs only when a
/// configured package could be affected.
pub fn missing_release_tag_errors(
	configuration: &WorkspaceConfiguration,
	discover_packages: impl FnOnce() -> MonochangeResult<Vec<PackageRecord>>,
) -> MonochangeResult<Vec<String>> {
	let candidates = configuration
		.packages
		.iter()
		.filter(|package| package.publish.enabled && package.publish.mode == PublishMode::Builtin)
		.filter(|package| {
			resolve_registry_kind(
				package.publish.registry.as_ref(),
				Ecosystem::from(package.package_type),
			)
			.is_ok_and(|registry| {
				registry_binds_trusted_publish_to_tag(registry, &package.publish.trusted_publishing)
			})
		})
		.filter(|package| {
			configuration
				.effective_release_identity(&package.id)
				.is_some_and(|identity| !identity.tag)
		})
		.collect::<Vec<_>>();

	if candidates.is_empty() {
		return Ok(Vec::new());
	}

	let packages = discover_packages()?;
	let packages_by_config_id = packages_by_config_id(&packages);

	Ok(candidates
		.into_iter()
		.filter(|package| {
			packages_by_config_id
				.get(package.id.as_str())
				.is_some_and(|record| package_can_be_published(package, record))
		})
		.map(|package| {
			missing_release_tag_message(
				&package.id,
				Some(&release_tag_owner_section(configuration, &package.id)),
			)
		})
		.collect())
}

/// Build the `dart pub token add` command that stores a pub.dev credential
/// referencing the `PUB_TOKEN` environment variable.
fn build_dart_pub_token_add_command(cwd: &Path) -> CommandSpec {
	CommandSpec {
		program: "dart".to_string(),
		args: vec![
			"pub".to_string(),
			"token".to_string(),
			"add".to_string(),
			PUB_DEV_AUDIENCE.to_string(),
			"--env-var".to_string(),
			PUB_TOKEN_ENV_VAR.to_string(),
		],
		cwd: cwd.to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

/// Mint a fresh GitHub Actions OIDC token right before the pub.dev publish
/// command runs and register it with the pub token store.
///
/// `dart-lang/setup-dart` performs this exchange once during workflow setup,
/// but GitHub OIDC tokens expire after five minutes and multi-ecosystem
/// publish runs can reach the dart step later than that. Minting here keeps
/// the mint-to-publish gap to seconds.
///
/// Steps:
///
/// 1. Detect the runner-provided OIDC endpoint (`ACTIONS_ID_TOKEN_REQUEST_URL`
///    plus `ACTIONS_ID_TOKEN_REQUEST_TOKEN`). Runs missing either variable,
///    for example outside GitHub Actions or without the ID-token permission,
///    are left untouched.
/// 2. Mint an audience-`https://pub.dev` JWT and export it as `PUB_TOKEN` in
///    the publish command's environment.
/// 3. Run `dart pub token add https://pub.dev --env-var PUB_TOKEN` so the pub
///    client reads the fresh token from `PUB_TOKEN` at publish time (the
///    client reads the env var per request and deletes stale credentials on
///    auth failures, so re-registering is idempotent and self-healing).
pub async fn refresh_pub_dev_trusted_publishing_token(
	client: &Client,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
	request: &PublishRequest,
	publish_command: &mut CommandSpec,
) -> MonochangeResult<()> {
	let Some(context) = actions_oidc_context(env_map) else {
		return Ok(());
	};

	let token = mint_actions_id_token(client, &context)
		.await
		.map_err(|error| {
			MonochangeError::Io(format!(
				"refreshing the pub.dev trusted publishing token for package `{}` failed: {}",
				request.package_name,
				error.render()
			))
		})?;

	publish_command
		.env
		.insert(PUB_TOKEN_ENV_VAR.to_string(), token.clone());

	let mut token_add_command = build_dart_pub_token_add_command(&request.package_root);
	token_add_command
		.env
		.insert(PUB_TOKEN_ENV_VAR.to_string(), token);
	let output = executor.run(&token_add_command)?;

	if !output.success {
		return Err(MonochangeError::Io(format!(
			"`{}` in {} failed: {}. monochange registers the fresh pub.dev OIDC token before every trusted publish, so the pub token store must be writable for the publishing user.",
			render_command(&token_add_command),
			token_add_command.cwd.display(),
			render_command_error(&output),
		)));
	}

	Ok(())
}

fn process_command_error(
	spec: &CommandSpec,
	action: &str,
	error: &std::io::Error,
) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to {action} `{}` in {}: {error}",
		render_command(spec),
		spec.cwd.display()
	))
}

fn tee_child_output(
	mut reader: impl std::io::Read,
	mut writer: impl std::io::Write,
) -> std::io::Result<Vec<u8>> {
	let mut captured = Vec::new();
	let mut buffer = [0; 8192];

	loop {
		let read = reader.read(&mut buffer)?;

		if read == 0 {
			break;
		}

		let (chunk, _) = buffer.split_at(read);
		writer.write_all(chunk)?;
		writer.flush()?;
		captured.extend_from_slice(chunk);
	}

	Ok(captured)
}

fn join_child_output(
	thread: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
	spec: &CommandSpec,
	stream_name: &str,
) -> MonochangeResult<Vec<u8>> {
	thread
		.join()
		.map_err(|_| {
			MonochangeError::Io(format!(
				"failed to collect {stream_name} from `{}` in {}: output reader panicked",
				render_command(spec),
				spec.cwd.display()
			))
		})?
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to collect {stream_name} from `{}` in {}: {error}",
				render_command(spec),
				spec.cwd.display()
			))
		})
}

pub fn render_command(spec: &CommandSpec) -> String {
	std::iter::once(spec.program.as_str())
		.chain(spec.args.iter().map(String::as_str))
		.collect::<Vec<_>>()
		.join(" ")
}

pub fn render_command_error(output: &CommandOutput) -> String {
	match (output.stdout.is_empty(), output.stderr.is_empty()) {
		(true, true) => "command failed without output".to_string(),
		(false, true) => output.stdout.clone(),
		(true, false) => output.stderr.clone(),
		(false, false) => format!("stdout:\n{}\n\nstderr:\n{}", output.stdout, output.stderr),
	}
}

/// A failed publish command, described by the registry's rate limit when that
/// is what stopped it, so the reason and retry time are not buried in tool logs.
fn failed_command_outcome(
	mode: PackagePublishRunMode,
	request: &PublishRequest,
	publish_command: &CommandSpec,
	output: &CommandOutput,
) -> PackagePublishOutcome {
	let now = chrono::Utc::now();
	let combined = format!("{}\n{}", output.stdout, output.stderr);
	let Some(hit) = detect_rate_limit_hit(request.registry, &combined, now) else {
		return failed_publish_outcome(
			mode,
			request,
			format!(
				"`{}` failed: {}",
				render_command(publish_command),
				render_publish_command_error(output, request, mode)
			),
		);
	};
	let mut outcome = failed_publish_outcome(
		mode,
		request,
		hit.render_message(request.registry.as_str(), now),
	);
	outcome.rate_limit = Some(hit);
	outcome
}

fn render_publish_command_error(
	output: &CommandOutput,
	request: &PublishRequest,
	mode: PackagePublishRunMode,
) -> String {
	let message = render_command_error(output);

	if is_npm_otp_error(output, request) {
		return format!("{message}\n\n{}", npm_otp_recovery_message(mode));
	}

	if is_pub_dev_auth_error(output, request) {
		return format!(
			"{message}\n\n{}",
			pub_dev_trusted_publishing_recovery_message()
		);
	}

	message
}

fn is_npm_otp_error(output: &CommandOutput, request: &PublishRequest) -> bool {
	request.registry == RegistryKind::Npm
		&& (output.stderr.contains("EOTP")
			|| output.stderr.contains("one-time password")
			|| output.stdout.contains("EOTP")
			|| output.stdout.contains("one-time password"))
}

fn npm_otp_recovery_message(mode: PackagePublishRunMode) -> &'static str {
	match mode {
		PackagePublishRunMode::Placeholder => {
			"npm requires a publish-time one-time password. Get the current OTP code from your npm authenticator, then rerun with `monochange publish placeholder --otp <CODE>`. You can also set `NPM_CONFIG_OTP=<CODE>` and `PNPM_CONFIG_OTP=<CODE>` for the command."
		}
		PackagePublishRunMode::Release => {
			"npm requires a publish-time one-time password. Get the current OTP code from your npm authenticator, then rerun the publish command with `NPM_CONFIG_OTP=<CODE>` and `PNPM_CONFIG_OTP=<CODE>` set in the environment."
		}
	}
}

fn staged_publish_message(request: &PublishRequest) -> String {
	format!(
		"staged {} {} on {}; it stays non-installable until a maintainer approves it with 2FA. Run `npm stage list` then `npm stage approve <stage-id>`, or use the Staged Packages tab on npmjs.com",
		request.package_name, request.version, request.registry
	)
}

fn is_pub_dev_auth_error(output: &CommandOutput, request: &PublishRequest) -> bool {
	if request.registry != RegistryKind::PubDev {
		return false;
	}

	let combined = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
	[
		"authentication",
		"authorization",
		"authorize",
		"credential",
		"credentials",
		"log in",
		"login",
		"oauth",
		"oidc",
		"pub token",
		"token add",
	]
	.iter()
	.any(|needle| combined.contains(needle))
}

fn pub_dev_trusted_publishing_recovery_message() -> &'static str {
	"pub.dev publishing could not authenticate non-interactively. If this package uses trusted publishing, verify the GitHub workflow has `id-token: write`, runs with the GitHub Actions environment configured on pub.dev, matches the package repository, and runs from a `refs/tags/*` ref matching the tag pattern configured on pub.dev (the tag must contain the published version). If using a token fallback, add it before publishing with `dart pub token add https://pub.dev --env-var PUB_TOKEN`."
}

pub fn build_publish_command(
	request: &PublishRequest,
	mode: PackagePublishRunMode,
	placeholder_dir: Option<&Path>,
	dry_run: bool,
) -> CommandSpec {
	build_publish_command_builder().build_publish_command(request, mode, placeholder_dir, dry_run)
}

pub fn build_publish_command_builder() -> PublishCommandBuilder {
	PublishCommandBuilder::new()
		.with_adapter(Box::new(NpmPublishAdapter))
		.with_adapter(Box::new(CargoPublishAdapter))
		.with_adapter(Box::new(DartPublishAdapter))
		.with_adapter(Box::new(JsrPublishAdapter))
		.with_adapter(Box::new(PythonPublishAdapter))
		.with_adapter(Box::new(GoPublishAdapter))
		.with_adapter(Box::new(CocoaPodsPublishAdapter))
		.with_adapter(Box::new(GradlePublishAdapter))
}

struct NpmPublishAdapter;

impl PublishAdapter for NpmPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::Npm
	}

	fn supports_provenance(&self) -> bool {
		true
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_npm_placeholder_publish_command(
			request,
			placeholder_path,
		))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_npm_release_publish_command(request))
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![CiProviderKind::GitHubActions, CiProviderKind::GitLabCi]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://docs.npmjs.com/trusted-publishers")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"npm trusted publishing supports GitHub Actions and GitLab CI/CD".to_string(),
			"monochange verifies npm trusted-publishing context and reports npm CLI trust repair commands, but real publish runs do not execute npm trust".to_string(),
		].to_vec()
	}
}

struct CargoPublishAdapter;

impl PublishAdapter for CargoPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::CratesIo
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_cargo_placeholder_publish_command(
			request,
			placeholder_path,
		))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_cargo_release_publish_command(request))
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![CiProviderKind::GitHubActions]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://crates.io/docs/trusted-publishing")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"crates.io trusted publishing uses OIDC short-lived tokens".to_string(),
			"monochange does not currently verify crates.io registry-side trusted-publisher setup"
				.to_string(),
		]
		.to_vec()
	}
}

struct DartPublishAdapter;

impl PublishAdapter for DartPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::PubDev
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_dart_publish_command(request, placeholder_path))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_dart_publish_command(request, &request.package_root))
	}

	fn append_dry_run_args(&self, args: &mut Vec<String>) {
		args.retain(|arg| arg != "--force");
		args.push("--dry-run".to_string());
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![
			CiProviderKind::GitHubActions,
			CiProviderKind::GoogleCloudBuild,
		]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://dart.dev/tools/pub/automated-publishing")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"pub.dev automated publishing uses configured OIDC publishers".to_string(),
			"pub.dev registry-side publisher setup requires manual review".to_string(),
		]
		.to_vec()
	}
}

struct JsrPublishAdapter;

impl PublishAdapter for JsrPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::Jsr
	}

	fn supports_provenance(&self) -> bool {
		true
	}

	fn build_placeholder_command(
		&self,
		_request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_jsr_publish_command(placeholder_path))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_jsr_publish_command(&request.package_root))
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![CiProviderKind::GitHubActions]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://jsr.io/docs/publishing-packages")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"JSR can publish from supported CI without long-lived tokens".to_string(),
			"JSR package provenance is available, but monochange does not verify registry-side setup".to_string(),
		].to_vec()
	}
}

struct PythonPublishAdapter;

impl PublishAdapter for PythonPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::Pypi
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_python_publish_command(request, placeholder_path))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_python_publish_command(request, &request.package_root))
	}

	fn append_dry_run_args(&self, _args: &mut Vec<String>) {}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![
			CiProviderKind::GitHubActions,
			CiProviderKind::GitLabCi,
			CiProviderKind::GoogleCloudBuild,
		]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://docs.pypi.org/trusted-publishers/")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"PyPI Trusted Publishers support multiple CI identity providers".to_string(),
			"PEP 740 digital attestations are separate from trusted-publisher authorization"
				.to_string(),
		]
		.to_vec()
	}
}

struct CocoaPodsPublishAdapter;

impl PublishAdapter for CocoaPodsPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::CocoaPodsTrunk
	}

	fn build_placeholder_command(
		&self,
		_request: &PublishRequest,
		_placeholder_path: &Path,
	) -> Option<CommandSpec> {
		None
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_cocoapods_publish_command(
			request,
			&request.package_root,
		))
	}

	fn append_dry_run_args(&self, args: &mut Vec<String>) {
		// `pod trunk push` has no dry-run flag; `pod spec lint` runs the same
		// validation locally without mutating the trunk.
		append_publish_dry_run_args(args, RegistryKind::CocoaPodsTrunk, true);
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://guides.cocoapods.org/making/getting-setup-with-trunk.html")
	}

	fn registry_notes(&self) -> Vec<String> {
		vec![
			"the CocoaPods trunk authenticates through per-machine session tokens, not trusted publishing".to_string(),
			"set `publish.trusted_publishing = false` and export COCOAPODS_TRUNK_TOKEN from a registered trunk session in CI".to_string(),
		]
	}
}

struct GradlePublishAdapter;

impl PublishAdapter for GradlePublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::MavenCentral
	}

	fn build_placeholder_command(
		&self,
		_request: &PublishRequest,
		_placeholder_path: &Path,
	) -> Option<CommandSpec> {
		// Placeholder publishing would burn the namespace verification a
		// Maven Central deployment needs; first releases publish for real.
		None
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_gradle_publish_command(request))
	}

	fn append_dry_run_args(&self, args: &mut Vec<String>) {
		append_publish_dry_run_args(args, RegistryKind::MavenCentral, true);
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://central.sonatype.org/publish/publish-portal/")
	}

	fn registry_notes(&self) -> Vec<String> {
		vec![
			"Maven Central has no trusted publishing: authenticate with a Central Portal user token through Gradle".to_string(),
			"set `publish.trusted_publishing = false` and provide mavenCentralUsername and mavenCentralPassword gradle properties (ORG_GRADLE_PROJECT_ environment variables) plus a signing key in CI".to_string(),
		]
	}
}

struct GoPublishAdapter;

impl PublishAdapter for GoPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::GoProxy
	}

	fn build_placeholder_command(
		&self,
		_request: &PublishRequest,
		_placeholder_path: &Path,
	) -> Option<CommandSpec> {
		None
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_go_publish_command(request))
	}

	fn append_dry_run_args(&self, _args: &mut Vec<String>) {}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		Vec::new()
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		None
	}

	fn registry_notes(&self) -> Vec<String> {
		["unknown registry capabilities are treated as unsupported".to_string()].to_vec()
	}
}

pub fn append_publish_dry_run_args(args: &mut Vec<String>, registry: RegistryKind, dry_run: bool) {
	if !dry_run {
		return;
	}

	if registry == RegistryKind::Pypi || registry == RegistryKind::GoProxy {
		return;
	}

	if registry == RegistryKind::CocoaPodsTrunk {
		for (arg, replacement) in args.iter_mut().take(2).zip(["spec", "lint"]) {
			*arg = replacement.to_string();
		}

		return;
	}

	if registry == RegistryKind::PubDev {
		args.retain(|arg| arg != "--force");
		args.push("--dry-run".to_string());

		return;
	}

	args.push("--dry-run".to_string());
}

pub fn build_npm_placeholder_publish_command(
	request: &PublishRequest,
	placeholder_path: &Path,
) -> CommandSpec {
	let mut args = vec![
		"publish".to_string(),
		placeholder_path.display().to_string(),
		"--access".to_string(),
		"public".to_string(),
	];
	append_pnpm_publish_args(request, &mut args);

	CommandSpec {
		program: npm_publish_program(request).to_string(),
		args,
		cwd: request.package_root.clone(),
		env: npm_publish_env(request),
		timeout: None,
	}
}

pub fn build_npm_release_publish_command(request: &PublishRequest) -> CommandSpec {
	let mut args = Vec::new();

	if request.flow == PublishFlow::Staged {
		// `npm stage publish` and `pnpm stage publish` share the same
		// subcommand shape and accept the same publish arguments.
		args.push("stage".to_string());
	}

	args.push("publish".to_string());
	args.push("--access".to_string());
	args.push("public".to_string());

	if request.attestations.require_registry_provenance {
		args.push("--provenance".to_string());
	}

	append_pnpm_publish_args(request, &mut args);

	CommandSpec {
		program: npm_publish_program(request).to_string(),
		args,
		cwd: request.package_root.clone(),
		env: npm_publish_env(request),
		timeout: None,
	}
}

/// Pick the CLI that publishes an npm-registry package.
///
/// pnpm workspaces always publish with pnpm, including under trusted
/// publishing: pnpm 11 performs the npm OIDC exchange natively, and only
/// pnpm rewrites `workspace:` dependency ranges into real versions when it
/// packs. Publishing a pnpm workspace with the npm CLI would ship those
/// ranges verbatim and break installs for consumers.
fn npm_publish_program(request: &PublishRequest) -> &'static str {
	if uses_pnpm_publish_manager(request) {
		"pnpm"
	} else {
		"npm"
	}
}

/// Skip pnpm's git checks for publishes monochange runs.
///
/// monochange validates the release commit before it publishes, and release
/// jobs routinely run from a detached tag checkout with build outputs in the
/// tree, which pnpm's clean-tree and publish-branch checks would reject.
fn append_pnpm_publish_args(request: &PublishRequest, args: &mut Vec<String>) {
	if uses_pnpm_publish_manager(request) {
		args.push("--no-git-checks".to_string());
	}
}

pub fn uses_pnpm_publish_manager(request: &PublishRequest) -> bool {
	request.registry == RegistryKind::Npm && request.package_manager.as_deref() == Some("pnpm")
}

fn build_cargo_placeholder_publish_command(
	request: &PublishRequest,
	placeholder_path: &Path,
) -> CommandSpec {
	CommandSpec {
		program: "cargo".to_string(),
		args: vec![
			"publish".to_string(),
			"--allow-dirty".to_string(),
			"--manifest-path".to_string(),
			placeholder_path.join("Cargo.toml").display().to_string(),
		],
		cwd: request.package_root.clone(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn build_cargo_release_publish_command(request: &PublishRequest) -> CommandSpec {
	CommandSpec {
		program: "cargo".to_string(),
		args: vec![
			"publish".to_string(),
			"--locked".to_string(),
			"--manifest-path".to_string(),
			request.manifest_path.display().to_string(),
		],
		cwd: request.package_root.clone(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn build_dart_publish_command(request: &PublishRequest, cwd: &Path) -> CommandSpec {
	let program = if request
		.package_metadata
		.get("is_flutter")
		.is_some_and(|v| v == "true")
	{
		"flutter"
	} else {
		"dart"
	};
	CommandSpec {
		program: program.to_string(),
		args: vec![
			"pub".to_string(),
			"publish".to_string(),
			"--force".to_string(),
		],
		cwd: cwd.to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn build_jsr_publish_command(cwd: &Path) -> CommandSpec {
	CommandSpec {
		program: "deno".to_string(),
		args: vec!["publish".to_string()],
		cwd: cwd.to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn build_python_publish_command(request: &PublishRequest, cwd: &Path) -> CommandSpec {
	let trusted_publishing = if request.trusted_publishing.enabled {
		"always"
	} else {
		"never"
	};

	let script = format!(
		"uv build --out-dir dist && uv publish --trusted-publishing {trusted_publishing} dist/*"
	);
	CommandSpec {
		program: "sh".to_string(),
		args: vec!["-c".to_string(), script],
		cwd: cwd.to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn build_go_publish_command(request: &PublishRequest) -> CommandSpec {
	CommandSpec {
		program: "git".to_string(),
		args: vec!["tag".to_string(), go_module_tag_name(request)],
		cwd: request.package_root.clone(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn go_module_tag_name(request: &PublishRequest) -> String {
	let version = go_proxy_version(&request.version);
	let root = request
		.package_metadata
		.get("relative_path")
		.cloned()
		.unwrap_or_else(|| fallback_go_tag_prefix(request));
	let root = root.trim_matches('/');

	if root.is_empty() || root == "." {
		return version;
	}

	format!("{root}/{version}")
}

fn fallback_go_tag_prefix(request: &PublishRequest) -> String {
	env::current_dir()
		.ok()
		.and_then(|root| {
			request
				.package_root
				.strip_prefix(root)
				.ok()
				.map(Path::to_path_buf)
		})
		.unwrap_or_else(|| request.package_root.clone())
		.to_string_lossy()
		.to_string()
}

fn build_cocoapods_publish_command(request: &PublishRequest, cwd: &Path) -> CommandSpec {
	let podspec_path = request
		.package_metadata
		.get("podspec_path")
		.cloned()
		.unwrap_or_else(|| {
			request.manifest_path.file_name().map_or_else(
				|| request.manifest_path.display().to_string(),
				|name| name.to_string_lossy().into_owned(),
			)
		});

	CommandSpec {
		program: "pod".to_string(),
		args: vec![
			"trunk".to_string(),
			"push".to_string(),
			podspec_path,
			"--allow-warnings".to_string(),
		],
		cwd: cwd.to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

fn build_gradle_publish_command(request: &PublishRequest) -> CommandSpec {
	const WRAPPER: &str = if cfg!(windows) {
		"gradlew.bat"
	} else {
		"./gradlew"
	};

	CommandSpec {
		// The Gradle wrapper resolves the project's publishing setup; the
		// vanniktech plugin's `publishAndReleaseToMavenCentral` task uploads
		// and releases the deployment in one run.
		program: WRAPPER.to_string(),
		args: vec!["publishAndReleaseToMavenCentral".to_string()],
		cwd: request.package_root.clone(),
		env: BTreeMap::new(),
		timeout: None,
	}
}

/// The Maven coordinate a Gradle publish request targets.
///
/// Prefers the `group_id` and `artifact_id` metadata the Gradle adapter
/// records; falls back to a `group:artifact` package name.
pub fn maven_central_coordinate(request: &PublishRequest) -> Option<(String, String)> {
	let group = request
		.package_metadata
		.get("group_id")
		.cloned()
		.or_else(|| {
			request
				.package_name
				.split_once(':')
				.map(|(group, _)| group.to_string())
		});
	let artifact = request
		.package_metadata
		.get("artifact_id")
		.cloned()
		.or_else(|| {
			request
				.package_name
				.split_once(':')
				.map(|(_, artifact)| artifact.to_string())
		});

	match (group, artifact) {
		(Some(group), Some(artifact)) if !group.is_empty() && !artifact.is_empty() => {
			Some((group, artifact))
		}

		_ => None,
	}
}
/// Render the Maven repository path for a group and artifact pair.
///
/// `dev.canvist` with artifact `canvist-android` becomes
/// `dev/canvist/canvist-android`.
fn maven_central_metadata_path(group_id: &str, artifact_id: &str) -> String {
	format!("{}/{artifact_id}", group_id.replace('.', "/"))
}

pub fn go_module_path(request: &PublishRequest) -> &str {
	request
		.package_metadata
		.get("module_path")
		.map_or(request.package_name.as_str(), String::as_str)
}

pub fn go_proxy_version(version: &str) -> String {
	if version.starts_with('v') {
		version.to_string()
	} else {
		format!("v{version}")
	}
}

pub fn go_proxy_module_path(module: &str) -> String {
	let mut escaped = String::with_capacity(module.len());

	for character in module.chars() {
		if character.is_ascii_uppercase() {
			escaped.push('!');
			escaped.push(character.to_ascii_lowercase());
		} else {
			escaped.push(character);
		}
	}

	escaped
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiProviderKind {
	GitHubActions,
	GitLabCi,
	CircleCi,
	GoogleCloudBuild,
	Unknown,
}

impl CiProviderKind {
	pub fn label(self) -> &'static str {
		match self {
			Self::GitHubActions => "GitHub Actions",
			Self::GitLabCi => "GitLab CI/CD",
			Self::CircleCi => "CircleCI",
			Self::GoogleCloudBuild => "Google Cloud Build",
			Self::Unknown => "unknown CI provider",
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum TrustedPublishingIdentity {
	GitHubActions {
		repository: Option<String>,
		workflow: Option<String>,
		workflow_ref: Option<String>,
		environment: Option<String>,
		ref_name: Option<String>,
		run_id: Option<String>,
	},
	GitLabCi {
		project_path: Option<String>,
		ref_name: Option<String>,
		pipeline_source: Option<String>,
		job_id: Option<String>,
	},
	CircleCi {
		project_slug: Option<String>,
		workflow_id: Option<String>,
		job_name: Option<String>,
	},
	GoogleCloudBuild {
		project_id: Option<String>,
		build_id: Option<String>,
		trigger_name: Option<String>,
		repository: Option<String>,
		ref_name: Option<String>,
	},
	Unknown {
		reason: String,
	},
}

impl TrustedPublishingIdentity {
	pub fn provider(&self) -> CiProviderKind {
		match self {
			Self::GitHubActions { .. } => CiProviderKind::GitHubActions,
			Self::GitLabCi { .. } => CiProviderKind::GitLabCi,
			Self::CircleCi { .. } => CiProviderKind::CircleCi,
			Self::GoogleCloudBuild { .. } => CiProviderKind::GoogleCloudBuild,
			Self::Unknown { .. } => CiProviderKind::Unknown,
		}
	}

	pub fn is_verifiable_by_env(&self) -> bool {
		match self {
			Self::GitHubActions {
				repository,
				workflow,
				workflow_ref,
				..
			} => repository.is_some() && (workflow.is_some() || workflow_ref.is_some()),
			Self::GitLabCi {
				project_path,
				job_id,
				..
			} => project_path.is_some() && job_id.is_some(),
			Self::CircleCi {
				project_slug,
				workflow_id,
				..
			} => project_slug.is_some() && workflow_id.is_some(),
			Self::GoogleCloudBuild {
				project_id,
				build_id,
				..
			} => project_id.is_some() && build_id.is_some(),
			Self::Unknown { .. } => false,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(
	clippy::struct_excessive_bools,
	reason = "capability matrix reports independent registry booleans"
)]
pub struct RegistryTrustCapabilities {
	pub registry: String,
	pub trusted_publishing: bool,
	pub supported_providers: Vec<CiProviderKind>,
	pub registry_setup_verifiable: bool,
	pub registry_setup_automation: bool,
	pub registry_native_provenance: bool,
	pub setup_url: Option<String>,
	pub notes: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(
	clippy::struct_excessive_bools,
	reason = "capability matrix reports independent provider/registry booleans"
)]
pub struct ProviderRegistryTrustCapability {
	pub registry: String,
	pub provider: CiProviderKind,
	pub trusted_publishing: bool,
	pub ci_identity_verifiable: bool,
	pub registry_setup_verifiable: bool,
	pub registry_setup_automation: bool,
	pub registry_native_provenance: bool,
	pub notes: Vec<String>,
}

pub fn detect_trusted_publishing_identity(
	env_map: &BTreeMap<String, String>,
) -> TrustedPublishingIdentity {
	if is_github_actions(env_map) {
		return TrustedPublishingIdentity::GitHubActions {
			repository: env_map.get("GITHUB_REPOSITORY").cloned(),
			workflow: env_map
				.get("GITHUB_WORKFLOW_REF")
				.and_then(|value| parse_github_workflow_ref(value))
				.or_else(|| env_map.get("GITHUB_WORKFLOW").cloned()),
			workflow_ref: env_map.get("GITHUB_WORKFLOW_REF").cloned(),
			environment: env_map
				.get("GITHUB_ENVIRONMENT")
				.or_else(|| env_map.get("MONOCHANGE_TRUSTED_PUBLISHING_ENVIRONMENT"))
				.cloned(),
			ref_name: env_map.get("GITHUB_REF_NAME").cloned(),
			run_id: env_map.get("GITHUB_RUN_ID").cloned(),
		};
	}

	if env_is_true(env_map, "GITLAB_CI") {
		return TrustedPublishingIdentity::GitLabCi {
			project_path: env_map.get("CI_PROJECT_PATH").cloned(),
			ref_name: env_map.get("CI_COMMIT_REF_NAME").cloned(),
			pipeline_source: env_map.get("CI_PIPELINE_SOURCE").cloned(),
			job_id: env_map.get("CI_JOB_ID").cloned(),
		};
	}

	if env_is_true(env_map, "CIRCLECI") {
		return TrustedPublishingIdentity::CircleCi {
			project_slug: circle_project_slug(env_map),
			workflow_id: env_map.get("CIRCLE_WORKFLOW_ID").cloned(),
			job_name: env_map.get("CIRCLE_JOB").cloned(),
		};
	}

	if env_map.contains_key("BUILD_ID")
		&& (env_map.contains_key("PROJECT_ID") || env_map.contains_key("GOOGLE_CLOUD_PROJECT"))
	{
		return TrustedPublishingIdentity::GoogleCloudBuild {
			project_id: env_map
				.get("PROJECT_ID")
				.or_else(|| env_map.get("GOOGLE_CLOUD_PROJECT"))
				.cloned(),
			build_id: env_map.get("BUILD_ID").cloned(),
			trigger_name: env_map.get("TRIGGER_NAME").cloned(),
			repository: env_map.get("REPO_NAME").cloned(),
			ref_name: env_map
				.get("BRANCH_NAME")
				.or_else(|| env_map.get("TAG_NAME"))
				.cloned(),
		};
	}

	TrustedPublishingIdentity::Unknown {
		reason: "no supported CI provider environment variables were detected".to_string(),
	}
}

pub fn registry_trust_capabilities(registry: &PublishRegistry) -> RegistryTrustCapabilities {
	match registry {
		PublishRegistry::Builtin(registry) => builtin_registry_trust_capabilities(*registry),
		PublishRegistry::Custom(name) => {
			RegistryTrustCapabilities {
				registry: name.clone(),
				trusted_publishing: false,
				supported_providers: Vec::new(),
				registry_setup_verifiable: false,
				registry_setup_automation: false,
				registry_native_provenance: false,
				setup_url: None,
				notes: vec![
				"custom/private registries have no built-in trusted-publishing contract in monochange"
					.to_string(),
			],
			}
		}
	}
}

pub fn builtin_registry_trust_capabilities(registry: RegistryKind) -> RegistryTrustCapabilities {
	let providers = supported_providers_for_registry(registry);
	RegistryTrustCapabilities {
		registry: registry.to_string(),
		trusted_publishing: !providers.is_empty(),
		supported_providers: providers,
		registry_setup_verifiable: registry == RegistryKind::Npm,
		registry_setup_automation: registry == RegistryKind::Npm,
		registry_native_provenance: matches!(
			registry,
			RegistryKind::Npm | RegistryKind::Jsr | RegistryKind::Pypi
		),
		setup_url: registry_setup_url(registry).map(str::to_string),
		notes: registry_notes(registry),
	}
}

pub fn provider_registry_trust_capability(
	registry: &PublishRegistry,
	provider: CiProviderKind,
) -> ProviderRegistryTrustCapability {
	let registry_capabilities = registry_trust_capabilities(registry);
	let supported = registry_capabilities
		.supported_providers
		.contains(&provider);

	let builtin = match registry {
		PublishRegistry::Builtin(registry) => Some(*registry),
		PublishRegistry::Custom(_) => None,
	};

	let registry_setup_verifiable = supported
		&& builtin == Some(RegistryKind::Npm)
		&& provider == CiProviderKind::GitHubActions;
	let registry_setup_automation = registry_setup_verifiable;
	let mut notes = registry_capabilities.notes.clone();

	if !supported {
		notes.push(format!(
			"{} is not a supported trusted-publishing provider for {}",
			provider.label(),
			registry_capabilities.registry
		));
	} else if !registry_setup_verifiable {
		notes.push(format!(
			"monochange can identify {} context for {}, but registry-side setup still requires manual review",
			provider.label(),
			registry_capabilities.registry
		));
	}

	ProviderRegistryTrustCapability {
		registry: registry_capabilities.registry,
		provider,
		trusted_publishing: supported,
		ci_identity_verifiable: supported && provider != CiProviderKind::Unknown,
		registry_setup_verifiable,
		registry_setup_automation,
		registry_native_provenance: supported && registry_capabilities.registry_native_provenance,
		notes,
	}
}

pub fn trusted_publishing_capability_message_for_builtin(
	registry: RegistryKind,
	env_map: &BTreeMap<String, String>,
) -> String {
	let identity = detect_trusted_publishing_identity(env_map);
	trusted_publishing_capability_message(&PublishRegistry::Builtin(registry), &identity)
}

pub fn trusted_publishing_capability_message(
	registry: &PublishRegistry,
	identity: &TrustedPublishingIdentity,
) -> String {
	let provider = identity.provider();
	let capability = provider_registry_trust_capability(registry, provider);
	let registry_capabilities = registry_trust_capabilities(registry);
	let supported_providers = provider_list(&registry_capabilities.supported_providers);

	if provider == CiProviderKind::Unknown {
		return format!(
			"No supported CI provider identity was detected for {} trusted publishing; supported providers: {}.",
			registry_capabilities.registry, supported_providers
		);
	}

	if !capability.trusted_publishing {
		return format!(
			"Current CI provider {} is not supported for {} trusted publishing; supported providers: {}.",
			provider.label(),
			capability.registry,
			supported_providers
		);
	}

	if !identity.is_verifiable_by_env() {
		return format!(
			"Current CI provider {} is supported for {} trusted publishing, but publish-time environment variables are incomplete; verify the registry publisher configuration manually.",
			provider.label(),
			capability.registry
		);
	}

	let registry_setup = if capability.registry_setup_verifiable {
		"monochange can verify registry-side setup"
	} else {
		"registry-side setup verification is manual"
	};
	let provenance = if capability.registry_native_provenance {
		"registry-native provenance is available"
	} else {
		"registry-native provenance is not available"
	};

	format!(
		"Current CI provider {} is supported for {} trusted publishing; {registry_setup}; {provenance}.",
		provider.label(),
		capability.registry
	)
}

fn supported_providers_for_registry(registry: RegistryKind) -> Vec<CiProviderKind> {
	build_publish_command_builder()
		.adapter_for_registry(registry)
		.map_or_else(Vec::new, PublishAdapter::supported_providers)
}

fn registry_setup_url(registry: RegistryKind) -> Option<&'static str> {
	build_publish_command_builder()
		.adapter_for_registry(registry)
		.and_then(PublishAdapter::registry_setup_url)
}

fn registry_notes(registry: RegistryKind) -> Vec<String> {
	build_publish_command_builder()
		.adapter_for_registry(registry)
		.map_or_else(
			|| vec!["unknown registry capabilities are treated as unsupported".to_string()],
			PublishAdapter::registry_notes,
		)
}

fn provider_list(providers: &[CiProviderKind]) -> String {
	if providers.is_empty() {
		return "none".to_string();
	}

	providers
		.iter()
		.map(|provider| provider.label())
		.collect::<Vec<_>>()
		.join(", ")
}

fn env_is_true(env_map: &BTreeMap<String, String>, key: &str) -> bool {
	env_map.get(key).is_some_and(|value| value == "true")
}

fn parse_github_workflow_ref(value: &str) -> Option<String> {
	let workflow_path = value.split('@').next()?;
	workflow_path
		.rsplit_once("/.github/workflows/")
		.map(|(_, workflow)| workflow.to_string())
}

fn circle_project_slug(env_map: &BTreeMap<String, String>) -> Option<String> {
	env_map
		.get("CIRCLE_PROJECT_USERNAME")
		.zip(env_map.get("CIRCLE_PROJECT_REPONAME"))
		.map(|(owner, repo)| format!("gh/{owner}/{repo}"))
		.or_else(|| env_map.get("CIRCLE_PROJECT_REPONAME").cloned())
}

use monochange_core::materialize_dependency_edges;

pub fn publish_order_dependency_edges(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> Vec<DependencyEdge> {
	let mut edges = materialize_dependency_edges(packages);
	let packages_by_name = packages
		.iter()
		.map(|package| (package.name.as_str(), package))
		.collect::<BTreeMap<_, _>>();

	for package in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Npm)
	{
		let parsed = fs::read_to_string(&package.manifest_path)
			.ok()
			.and_then(|contents| serde_json::from_str::<JsonValue>(&contents).ok());
		let Some(JsonValue::Object(root)) = parsed else {
			continue;
		};

		let declared_fields = package
			.declared_dependencies
			.iter()
			.filter_map(|dependency| dependency.source_field.as_deref())
			.collect::<BTreeSet<_>>();

		for field in publish_order_dependency_fields(configuration, package.ecosystem) {
			if declared_fields.contains(field.as_str()) {
				continue;
			}

			let Some(JsonValue::Object(dependencies)) = root.get(&field) else {
				continue;
			};

			for (dependency_name, constraint) in dependencies {
				let Some(target) = packages_by_name.get(dependency_name.as_str()) else {
					continue;
				};
				edges.push(DependencyEdge {
					from_package_id: package.id.clone(),
					to_package_id: target.id.clone(),
					dependency_kind: DependencyKind::Unknown,
					source_kind: DependencySourceKind::Manifest,
					version_constraint: constraint.as_str().map(ToString::to_string),
					is_optional: false,
					is_direct: true,
					source_field: Some(field.clone()),
				});
			}
		}
	}

	edges
}

pub fn publish_order_dependency_fields(
	configuration: &WorkspaceConfiguration,
	ecosystem: Ecosystem,
) -> BTreeSet<String> {
	let settings = ecosystem_settings(configuration, ecosystem);
	settings
		.publish_order
		.dependency_fields
		.clone()
		.unwrap_or_else(|| {
			default_publish_order_dependency_fields(ecosystem)
				.iter()
				.map(|field| (*field).to_string())
				.collect()
		})
		.into_iter()
		.collect()
}

fn ecosystem_settings(
	configuration: &WorkspaceConfiguration,
	ecosystem: Ecosystem,
) -> &EcosystemSettings {
	match ecosystem {
		Ecosystem::Cargo => &configuration.cargo,
		Ecosystem::Deno => &configuration.deno,
		Ecosystem::Dart => &configuration.dart,
		Ecosystem::Python => &configuration.python,
		Ecosystem::Go => &configuration.go,
		Ecosystem::Swift => &configuration.swift,
		Ecosystem::CocoaPods => &configuration.cocoapods,
		Ecosystem::Gradle => &configuration.gradle,
		_ => &configuration.npm,
	}
}

pub fn read_publish_report_artifact(path: &Path) -> MonochangeResult<PackagePublishReport> {
	let body = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read package publish resume artifact {}: {error}",
			path.display()
		))
	})?;
	serde_json::from_str(&body).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse package publish resume artifact {}: {error}",
			path.display()
		))
	})
}

pub fn write_publish_report_artifact(
	path: &Path,
	report: &PackagePublishReport,
) -> MonochangeResult<()> {
	path.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(create_publish_report_directory)
		.transpose()?;
	let body = serde_json::to_string_pretty(report).map_err(publish_report_json_error)?;
	fs::write(path, format!("{body}\n")).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write package publish output {}: {error}",
			path.display()
		))
	})
}

pub fn ensure_publish_report_succeeded(report: &PackagePublishReport) -> MonochangeResult<()> {
	ensure_publish_report_succeeded_at(report, chrono::Utc::now())
}

fn ensure_publish_report_succeeded_at(
	report: &PackagePublishReport,
	now: chrono::DateTime<chrono::Utc>,
) -> MonochangeResult<()> {
	let summary = report.summary();
	let Some(failed) = report
		.packages
		.iter()
		.find(|outcome| outcome.status == PackagePublishStatus::Failed)
	else {
		return Ok(());
	};

	if let Some(hit) = &failed.rate_limit {
		return Err(MonochangeError::Discovery(rate_limited_publish_message(
			failed, hit, summary, now,
		)));
	}

	// Packages left to other runs are listed only when there are some, so
	// runs without tag-bound packages report as before.
	let mut left_to_other_runs = String::new();
	for (count, label) in [
		(summary.deferred, "deferred to release tags"),
		(summary.out_of_scope, "out of scope for this ref"),
	] {
		if count > 0 {
			let _ = write!(left_to_other_runs, ", {count} {label}");
		}
	}

	Err(MonochangeError::Discovery(format!(
		"package publishing did not complete: {} total, {} published, {} failed, {} blocked, {} already existed, {} not attempted, {} planned{left_to_other_runs}; failed package {} {}: {}",
		summary.total(),
		summary.published,
		summary.failed,
		summary.blocked,
		summary.already_exists,
		summary.not_attempted,
		summary.planned,
		failed.package,
		failed.version,
		failed.message
	)))
}

/// Explain a run stopped by a registry rate limit: what was published, the
/// registry's reason, when uploads reopen, and how to resume.
///
/// Lines starting with `help: ` become the diagnostic's hints.
fn rate_limited_publish_message(
	failed: &PackagePublishOutcome,
	hit: &PublishRateLimitHit,
	summary: PackagePublishSummary,
	now: chrono::DateTime<chrono::Utc>,
) -> String {
	let registry = registry_display_name(&failed.registry);
	// Packages deferred or out of scope belong to other runs, so they are
	// neither this run's total nor left for its rerun.
	let total = summary.this_run();
	let done = summary.published + summary.staged + summary.already_exists;
	let left = total - done;
	let retry_at = hit.render_retry_at(now);
	let mut lines = vec![format!(
		"{registry} rate limit stopped publishing at {} {}: {done} of {total} packages published",
		failed.package, failed.version,
	)];
	lines.extend(
		hit.reason
			.as_ref()
			.map(|reason| format!("{registry}: {reason}")),
	);
	lines.extend(retry_at.as_ref().map(|time| format!("retry after: {time}")));
	lines.extend(hit.docs_url.as_ref().map(|url| format!("limits: {url}")));
	let wait = retry_at.map_or_else(
		|| format!("Wait for the {registry} rate limit to reset"),
		|time| format!("Wait until {time}"),
	);
	let packages = if left == 1 { "package" } else { "packages" };
	lines.push(format!(
		"help: {wait}, then rerun the same command. Versions that are already published are \
		 skipped, so publishing resumes with {} ({left} {packages} left).",
		failed.package
	));
	lines.join("\n")
}

pub fn create_publish_report_directory(parent: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create package publish output directory {}: {error}",
			parent.display()
		))
	})
}

pub fn publish_report_json_error(error: impl std::fmt::Display) -> MonochangeError {
	MonochangeError::Config(format!(
		"failed to serialize package publish report: {error}"
	))
}

type PublishResumeKey = (String, String, String);

pub fn resume_publish_requests(
	requests: &[PublishRequest],
	previous_report: Option<&PackagePublishReport>,
) -> MonochangeResult<(Vec<PublishRequest>, Vec<PackagePublishOutcome>)> {
	let Some(previous_report) = previous_report else {
		return Ok((requests.to_vec(), Vec::new()));
	};
	validate_resume_report(previous_report)?;

	let request_keys = requests
		.iter()
		.map(publish_request_resume_key)
		.collect::<BTreeSet<_>>();
	let completed_keys = previous_report
		.packages
		.iter()
		.filter(|outcome| package_publish_status_is_resumable_complete(outcome.status))
		.map(package_publish_outcome_resume_key)
		.collect::<BTreeSet<_>>();
	let resumed_outcomes = previous_report
		.packages
		.iter()
		.filter(|outcome| {
			request_keys.contains(&package_publish_outcome_resume_key(outcome))
				&& package_publish_status_is_resumable_complete(outcome.status)
		})
		.cloned()
		.collect::<Vec<_>>();
	let pending_requests = requests
		.iter()
		.filter(|request| !completed_keys.contains(&publish_request_resume_key(request)))
		.cloned()
		.collect::<Vec<_>>();

	Ok((pending_requests, resumed_outcomes))
}

pub fn validate_resume_report(report: &PackagePublishReport) -> MonochangeResult<()> {
	if report.mode != PackagePublishRunMode::Release {
		return Err(MonochangeError::Config(
			"package publish resume artifact must come from `monochange publish packages`"
				.to_string(),
		));
	}

	if report.dry_run {
		return Err(MonochangeError::Config(
			"package publish resume artifact must come from a real publish run".to_string(),
		));
	}

	Ok(())
}

pub fn publish_request_resume_key(request: &PublishRequest) -> PublishResumeKey {
	(
		request.package_id.clone(),
		request.registry.to_string(),
		request.version.clone(),
	)
}

pub fn package_publish_outcome_resume_key(outcome: &PackagePublishOutcome) -> PublishResumeKey {
	(
		outcome.package.clone(),
		outcome.registry.clone(),
		outcome.version.clone(),
	)
}

/// Whether a resumed run keeps an earlier outcome instead of retrying it.
///
/// Packages an earlier run left to other runs (deferred or out of scope) were
/// never published, so a resumed run plans them again for its own ref: a
/// resume on the same ref leaves them out again without a registry request,
/// and a resume on another ref can publish them.
pub fn package_publish_status_is_resumable_complete(status: PackagePublishStatus) -> bool {
	matches!(
		status,
		PackagePublishStatus::Published
			| PackagePublishStatus::Staged
			| PackagePublishStatus::SkippedExisting
			| PackagePublishStatus::SkippedExternal
	)
}

pub fn merge_publish_resume_report(
	mode: PackagePublishRunMode,
	dry_run: bool,
	mut resumed_outcomes: Vec<PackagePublishOutcome>,
	mut current_report: PackagePublishReport,
) -> PackagePublishReport {
	if resumed_outcomes.is_empty() {
		return current_report;
	}

	resumed_outcomes.append(&mut current_report.packages);
	PackagePublishReport {
		mode,
		dry_run,
		packages: resumed_outcomes,
	}
}

pub fn order_release_requests_by_publish_dependencies(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	mut requests: Vec<PublishRequest>,
) -> MonochangeResult<Vec<PublishRequest>> {
	requests.sort_by(|left, right| {
		left.package_id
			.cmp(&right.package_id)
			.then_with(|| left.registry.to_string().cmp(&right.registry.to_string()))
			.then_with(|| left.version.cmp(&right.version))
	});

	let mut requests_by_package = BTreeMap::<String, Vec<PublishRequest>>::new();

	for request in requests {
		requests_by_package
			.entry(request.package_id.clone())
			.or_default()
			.push(request);
	}

	let request_ids = requests_by_package.keys().cloned().collect::<BTreeSet<_>>();
	let config_ids_by_record_id = config_ids_by_package_record_id(packages);
	let mut dependencies_by_package = request_ids
		.iter()
		.map(|package_id| (package_id.clone(), BTreeSet::<String>::new()))
		.collect::<BTreeMap<_, _>>();
	let mut dependents_by_package = BTreeMap::<String, BTreeSet<String>>::new();

	for edge in publish_order_dependency_edges(configuration, packages) {
		let from_package = packages
			.iter()
			.find(|package| package.id == edge.from_package_id)
			.expect("publish-order edges originate from discovered packages");

		if !publish_order_dependency_fields(configuration, from_package.ecosystem)
			.contains(edge.source_field.as_deref().unwrap_or_default())
		{
			continue;
		}
		let Some(from_package_id) = config_ids_by_record_id.get(&edge.from_package_id) else {
			continue;
		};
		let Some(to_package_id) = config_ids_by_record_id.get(&edge.to_package_id) else {
			continue;
		};

		if !request_ids.contains(from_package_id) || !request_ids.contains(to_package_id) {
			continue;
		}

		dependencies_by_package
			.entry(from_package_id.clone())
			.or_default()
			.insert(to_package_id.clone());
		dependents_by_package
			.entry(to_package_id.clone())
			.or_default()
			.insert(from_package_id.clone());
	}

	let mut ready = dependencies_by_package
		.iter()
		.filter(|&(_package_id, dependencies)| dependencies.is_empty())
		.map(|(package_id, _dependencies)| package_id.clone())
		.collect::<BTreeSet<_>>();
	let mut ordered_package_ids = Vec::with_capacity(dependencies_by_package.len());

	while let Some(package_id) = ready.iter().next().cloned() {
		ready.remove(&package_id);
		ordered_package_ids.push(package_id.clone());
		dependencies_by_package.remove(&package_id);

		let Some(dependents) = dependents_by_package.get(&package_id).cloned() else {
			continue;
		};

		for dependent_package_id in dependents {
			let dependencies = dependencies_by_package
				.get_mut(&dependent_package_id)
				.expect(
					"dependent package must remain pending until its dependencies are published",
				);
			dependencies.remove(&package_id);

			if dependencies.is_empty() {
				ready.insert(dependent_package_id);
			}
		}
	}

	if !dependencies_by_package.is_empty() {
		return Err(MonochangeError::Config(format!(
			"cyclic publish dependencies detected among package publications: {}",
			render_publish_dependency_cycle(&dependencies_by_package)
		)));
	}

	let mut ordered_requests = Vec::new();

	for package_id in ordered_package_ids {
		let mut package_requests = requests_by_package
			.remove(&package_id)
			.expect("ordered package ids must come from publish requests");
		ordered_requests.append(&mut package_requests);
	}

	Ok(ordered_requests)
}

pub fn config_ids_by_package_record_id(packages: &[PackageRecord]) -> BTreeMap<String, String> {
	packages
		.iter()
		.map(|package| {
			let config_id = package
				.metadata
				.get("config_id")
				.map_or(package.name.as_str(), String::as_str);
			(package.id.clone(), config_id.to_string())
		})
		.collect()
}

pub fn render_publish_dependency_cycle(
	dependencies_by_package: &BTreeMap<String, BTreeSet<String>>,
) -> String {
	let cycle_edges = dependencies_by_package
		.iter()
		.flat_map(|(package_id, dependencies)| {
			dependencies
				.iter()
				.map(move |dependency_id| format!("{package_id} -> {dependency_id}"))
		})
		.collect::<Vec<_>>();
	cycle_edges.join(", ")
}

pub fn packages_by_config_id(packages: &[PackageRecord]) -> BTreeMap<&str, &PackageRecord> {
	packages
		.iter()
		.map(|package| {
			let config_id = package
				.metadata
				.get("config_id")
				.map_or(package.name.as_str(), String::as_str);
			(config_id, package)
		})
		.collect()
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RegistryEndpoints {
	pub npm_registry: String,
	pub crates_io_api: String,
	pub crates_io_index: String,
	pub pub_dev_api: String,
	pub jsr_base: String,
	pub pypi_api: String,
	pub go_proxy: String,
	pub cocoapods_trunk_api: String,
	pub maven_central: String,
}

impl RegistryEndpoints {
	pub fn from_env() -> Self {
		Self {
			npm_registry: env::var("MONOCHANGE_NPM_REGISTRY_URL")
				.unwrap_or_else(|_| "https://registry.npmjs.org".to_string()),
			crates_io_api: env::var("MONOCHANGE_CRATES_IO_API_URL")
				.unwrap_or_else(|_| "https://crates.io/api/v1".to_string()),
			crates_io_index: env::var("MONOCHANGE_CRATES_IO_INDEX_URL")
				.unwrap_or_else(|_| "https://index.crates.io".to_string()),
			pub_dev_api: env::var("MONOCHANGE_PUB_DEV_API_URL")
				.unwrap_or_else(|_| "https://pub.dev/api".to_string()),
			jsr_base: env::var("MONOCHANGE_JSR_BASE_URL")
				.unwrap_or_else(|_| "https://jsr.io".to_string()),
			pypi_api: env::var("MONOCHANGE_PYPI_API_URL")
				.unwrap_or_else(|_| "https://pypi.org/pypi".to_string()),
			go_proxy: env::var("MONOCHANGE_GO_PROXY_URL")
				.unwrap_or_else(|_| "https://proxy.golang.org".to_string()),
			cocoapods_trunk_api: env::var("MONOCHANGE_COCOAPODS_TRUNK_API_URL")
				.unwrap_or_else(|_| "https://trunk.cocoapods.org/api/v1".to_string()),
			maven_central: env::var("MONOCHANGE_MAVEN_CENTRAL_URL")
				.unwrap_or_else(|_| "https://repo1.maven.org/maven2".to_string()),
		}
	}
}

pub fn registry_client() -> MonochangeResult<Client> {
	let _ = ring_provider().install_default();

	Client::builder()
		.user_agent(format!("monochange/{}", env!("CARGO_PKG_VERSION")))
		// Registry lookups must never hang indefinitely: a registry that accepts
		// a connection but never answers would otherwise stall publish runs and
		// readiness checks forever.
		.connect_timeout(Duration::from_secs(30))
		.timeout(Duration::from_secs(60))
		.build()
		.map_err(http_error("registry client build"))
}
pub fn package_can_be_published(
	package_definition: &monochange_core::PackageDefinition,
	package: &PackageRecord,
) -> bool {
	package_definition.publish.enabled
		&& !matches!(
			package.publish_state,
			PublishState::Private | PublishState::Excluded
		)
}
pub async fn filter_pending_publish_requests(
	requests: &[PublishRequest],
) -> MonochangeResult<Vec<PublishRequest>> {
	let client = registry_client()?;
	let endpoints = RegistryEndpoints::from_env();
	filter_pending_publish_requests_with_transport(requests, &client, &endpoints).await
}
pub async fn filter_pending_publish_requests_with_transport(
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
) -> MonochangeResult<Vec<PublishRequest>> {
	/// How many registry probes run at once. Probing is read-only, but a
	/// release with dozens of packages must not open every connection to one
	/// registry at the same time.
	const PROBE_CONCURRENCY: usize = 8;

	let mut pending_requests = Vec::with_capacity(requests.len());

	for chunk in requests.chunks(PROBE_CONCURRENCY) {
		// Spawn the whole chunk before awaiting any of it, so the probes
		// overlap; awaiting the handles in chunk order keeps the result in
		// the input order the sequential loop produced.
		let mut probes = Vec::with_capacity(chunk.len());
		for request in chunk {
			if request.mode == PublishMode::External {
				probes.push(None);
				continue;
			}

			let client = client.clone();
			let endpoints = endpoints.clone();
			let request = request.clone();
			probes.push(Some(tokio::spawn(async move {
				registry_version_exists(&client, &endpoints, &request).await
			})));
		}

		for (request, probe) in chunk.iter().zip(probes) {
			let Some(probe) = probe else {
				continue;
			};
			// The spawned body is an HTTP round trip that reports failures
			// through its own result; a join error would mean the task
			// itself panicked, which the probe body cannot do.
			let exists = probe.await.unwrap_or_else(|join_error| {
				// patch-coverage:ignore-start -- the probe body reports its own
				// failures through its result; a join error would require the
				// runtime itself to fail and cannot be triggered by a test.
				panic!("registry existence probe task should not panic: {join_error}")
				// patch-coverage:ignore-end
			})?;

			if exists {
				continue;
			}

			pending_requests.push(request.clone());
		}
	}

	Ok(pending_requests)
}

/// Probe whether a package exists on its registry at all (any version).
///
/// Returns `Ok(None)` for registries that monochange cannot probe without
/// credentials. Trusted publishing requires an existing package on npm,
/// crates.io, and pub.dev, so readiness uses this to catch packages that
/// would fail midway through a publish run.
pub async fn registry_package_exists_with_transport(
	request: &PublishRequest,
	client: &Client,
	endpoints: &RegistryEndpoints,
) -> MonochangeResult<Option<bool>> {
	if request.registry == RegistryKind::Npm {
		let url = format!(
			"{}/{}",
			endpoints.npm_registry.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("npm registry lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(Some(false));
		}

		let response = response
			.error_for_status()
			.map_err(http_error("npm registry lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("npm registry decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_object)
			.is_some_and(|versions| !versions.is_empty());

		return Ok(Some(exists));
	}

	if request.registry == RegistryKind::CratesIo {
		let url = format!(
			"{}/crates/{}",
			endpoints.crates_io_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("crates.io lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(Some(false));
		}

		let response = response
			.error_for_status()
			.map_err(http_error("crates.io lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("crates.io decode"))?;
		let exists = json.get("crate").is_some();

		return Ok(Some(exists));
	}

	if request.registry == RegistryKind::PubDev {
		let url = format!(
			"{}/packages/{}",
			endpoints.pub_dev_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("pub.dev lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(Some(false));
		}

		let response = response
			.error_for_status()
			.map_err(http_error("pub.dev lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("pub.dev decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_array)
			.is_some_and(|versions| !versions.is_empty());

		return Ok(Some(exists));
	}

	Ok(None)
}
pub async fn registry_version_exists(
	client: &Client,
	endpoints: &RegistryEndpoints,
	request: &PublishRequest,
) -> MonochangeResult<bool> {
	if request.registry == RegistryKind::Npm {
		let url = format!(
			"{}/{}",
			endpoints.npm_registry.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("npm registry lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("npm registry lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("npm registry decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_object)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.contains_key(&request.version)
			});

		return Ok(exists);
	}

	if request.registry == RegistryKind::CratesIo {
		return crates_io_version_exists(client, endpoints, request).await;
	}

	if request.registry == RegistryKind::PubDev {
		let url = format!(
			"{}/packages/{}",
			endpoints.pub_dev_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("pub.dev lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("pub.dev lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("pub.dev decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_array)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.iter().any(|version| {
						version.get("version").and_then(JsonValue::as_str)
							== Some(request.version.as_str())
					})
			});

		return Ok(exists);
	}

	if request.registry == RegistryKind::Pypi {
		let url = format!(
			"{}/{}/json",
			endpoints.pypi_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("PyPI lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("PyPI lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("PyPI decode"))?;
		let exists = json
			.get("releases")
			.and_then(JsonValue::as_object)
			.is_some_and(|releases| {
				request.placeholder && !releases.is_empty()
					|| releases.contains_key(&request.version)
			});

		return Ok(exists);
	}

	if request.registry == RegistryKind::CocoaPodsTrunk {
		let url = format!(
			"{}/pods/{}",
			endpoints.cocoapods_trunk_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("cocoapods trunk lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("cocoapods trunk lookup"))?;
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("cocoapods trunk decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_array)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.iter().any(|version| {
						version.get("name").and_then(JsonValue::as_str)
							== Some(request.version.as_str())
					})
			});

		return Ok(exists);
	}

	if request.registry == RegistryKind::MavenCentral {
		let Some((group, artifact)) = maven_central_coordinate(request) else {
			return Err(MonochangeError::Config(format!(
				"gradle package `{}` has no Maven coordinate; set `GROUP` (or `POM_GROUP_ID`) and `POM_ARTIFACT_ID` in gradle.properties",
				request.package_name
			)));
		};
		let url = format!(
			"{}/{}/maven-metadata.xml",
			endpoints.maven_central.trim_end_matches('/'),
			maven_central_metadata_path(&group, &artifact)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("maven central lookup"))?;

		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("maven central lookup"))?;
		let body = response
			.text()
			.await
			.map_err(http_error("maven central decode"))?;
		let needle = format!("<version>{}</version>", request.version);
		let exists = request.placeholder && body.contains("<version>") || body.contains(&needle);

		return Ok(exists);
	}

	if request.registry == RegistryKind::GoProxy {
		let url = format!(
			"{}/{}/@v/{}.info",
			endpoints.go_proxy.trim_end_matches('/'),
			go_proxy_module_path(go_module_path(request)),
			go_proxy_version(&request.version)
		);
		let response = client
			.get(url)
			.send()
			.await
			.map_err(http_error("Go proxy version lookup"))?;

		if response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::GONE {
			return Ok(false);
		}

		response
			.error_for_status()
			.map_err(http_error("Go proxy version lookup"))?;

		return Ok(true);
	}

	let url = format!(
		"{}/{}/meta.json",
		endpoints.jsr_base.trim_end_matches('/'),
		request.package_name
	);
	let response = client
		.get(url)
		.send()
		.await
		.map_err(http_error("jsr lookup"))?;

	if response.status() == StatusCode::NOT_FOUND {
		return Ok(false);
	}

	let response = response
		.error_for_status()
		.map_err(http_error("jsr lookup"))?;
	let json = response
		.json::<JsonValue>()
		.await
		.map_err(http_error("jsr decode"))?;
	let exists = json
		.get("versions")
		.and_then(JsonValue::as_object)
		.is_some_and(|versions| {
			request.placeholder && !versions.is_empty() || versions.contains_key(&request.version)
		});
	Ok(exists)
}
pub async fn crates_io_version_exists(
	client: &Client,
	endpoints: &RegistryEndpoints,
	request: &PublishRequest,
) -> MonochangeResult<bool> {
	let url = format!(
		"{}/crates/{}",
		endpoints.crates_io_api.trim_end_matches('/'),
		encode(&request.package_name)
	);
	let response = client
		.get(url)
		.send()
		.await
		.map_err(http_error("crates.io lookup"))?;
	let status = response.status();

	if status == StatusCode::NOT_FOUND {
		return Ok(false);
	}

	if status.is_success() {
		let json = response
			.json::<JsonValue>()
			.await
			.map_err(http_error("crates.io decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_array)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.iter().any(|version| {
						version.get("num").and_then(JsonValue::as_str)
							== Some(request.version.as_str())
					})
			});

		return Ok(exists);
	}

	crates_io_index_version_exists(client, endpoints, request)
		.await
		.map_err(|error| {
			MonochangeError::Discovery(format!(
				"crates.io lookup failed with http status {status}; crates.io index fallback failed: {error}"
			))
		})
}
pub async fn crates_io_index_version_exists(
	client: &Client,
	endpoints: &RegistryEndpoints,
	request: &PublishRequest,
) -> MonochangeResult<bool> {
	let url = format!(
		"{}/{}",
		endpoints.crates_io_index.trim_end_matches('/'),
		crates_io_index_entry_path(&request.package_name)
	);
	let response = client
		.get(url)
		.send()
		.await
		.map_err(http_error("crates.io index lookup"))?;

	if response.status() == StatusCode::NOT_FOUND {
		return Ok(false);
	}

	let response = response
		.error_for_status()
		.map_err(http_error("crates.io index lookup"))?;
	let body = response
		.text()
		.await
		.map_err(http_error("crates.io index decode"))?;

	for line in body.lines().filter(|line| !line.trim().is_empty()) {
		let entry = serde_json::from_str::<JsonValue>(line).map_err(|error| {
			MonochangeError::Discovery(format!("crates.io index decode failed: {error}"))
		})?;
		let Some(version) = entry.get("vers").and_then(JsonValue::as_str) else {
			continue;
		};

		if request.placeholder || version == request.version {
			return Ok(true);
		}
	}

	Ok(false)
}
pub fn crates_io_index_entry_path(package_name: &str) -> String {
	let normalized = package_name.to_ascii_lowercase();

	match normalized.len() {
		0 => String::new(),
		1 => format!("1/{normalized}"),
		2 => format!("2/{normalized}"),
		3 => format!("3/{}/{normalized}", &normalized[..1]),
		_ => format!("{}/{}/{}", &normalized[..2], &normalized[2..4], normalized),
	}
}
fn http_error(context: &'static str) -> impl Fn(reqwest::Error) -> MonochangeError {
	move |error| MonochangeError::Discovery(format!("{context} failed: {error}"))
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
