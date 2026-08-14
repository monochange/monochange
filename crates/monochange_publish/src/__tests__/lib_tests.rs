#![allow(clippy::disallowed_methods)]

use monochange_core::ChangelogSettings;
use monochange_core::ChangesetSettings;
use monochange_core::DependencyKind;
use monochange_core::GroupChangelogInclude;
use monochange_core::PackageDependency;
use monochange_core::PublishOrderSettings;
use monochange_core::VersionFormat;
use monochange_core::WorkspaceDefaults;
use monochange_core::lint::WorkspaceLintSettings;

use super::*;

fn empty_configuration(root: &Path) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	}
}

fn builtin_provider_registry_trust_capability(
	registry: RegistryKind,
	provider: CiProviderKind,
) -> ProviderRegistryTrustCapability {
	provider_registry_trust_capability(&PublishRegistry::Builtin(registry), provider)
}

#[tokio::test(flavor = "multi_thread")]
async fn build_pending_configured_package_release_requests_handles_empty_configuration() {
	let configuration = empty_configuration(Path::new("."));
	let requests =
		build_pending_configured_package_release_requests(&configuration, &[], &BTreeSet::new())
			.await
			.unwrap_or_else(|error| panic!("build pending configured release requests: {error}"));

	assert!(requests.is_empty());
}

#[test]
fn detects_github_actions_identity_from_workflow_ref() {
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_RUN_ID".to_string(), "123".to_string()),
	]);

	let identity = detect_trusted_publishing_identity(&env_map);

	assert_eq!(identity.provider(), CiProviderKind::GitHubActions);
	assert!(identity.is_verifiable_by_env());
	assert!(matches!(
		identity,
		TrustedPublishingIdentity::GitHubActions {
			workflow: Some(workflow),
			..
		} if workflow == "publish.yml"
	));
}

#[test]
fn detects_gitlab_circleci_and_google_cloud_build_identities() {
	let gitlab = detect_trusted_publishing_identity(&BTreeMap::from([
		("GITLAB_CI".to_string(), "true".to_string()),
		(
			"CI_PROJECT_PATH".to_string(),
			"monochange/monochange".to_string(),
		),
		("CI_JOB_ID".to_string(), "42".to_string()),
	]));
	assert_eq!(gitlab.provider(), CiProviderKind::GitLabCi);
	assert!(gitlab.is_verifiable_by_env());

	let circle = detect_trusted_publishing_identity(&BTreeMap::from([
		("CIRCLECI".to_string(), "true".to_string()),
		(
			"CIRCLE_PROJECT_USERNAME".to_string(),
			"monochange".to_string(),
		),
		(
			"CIRCLE_PROJECT_REPONAME".to_string(),
			"monochange".to_string(),
		),
		("CIRCLE_WORKFLOW_ID".to_string(), "workflow".to_string()),
	]));
	assert_eq!(circle.provider(), CiProviderKind::CircleCi);
	assert!(circle.is_verifiable_by_env());

	let google = detect_trusted_publishing_identity(&BTreeMap::from([
		("BUILD_ID".to_string(), "build".to_string()),
		("PROJECT_ID".to_string(), "project".to_string()),
		("REPO_NAME".to_string(), "monochange".to_string()),
	]));
	assert_eq!(google.provider(), CiProviderKind::GoogleCloudBuild);
	assert!(google.is_verifiable_by_env());
}

#[test]
fn builtin_registry_matrix_lists_supported_providers_without_overstating_setup_verification() {
	let npm = builtin_registry_trust_capabilities(RegistryKind::Npm);
	assert!(npm.trusted_publishing);
	assert_eq!(
		npm.supported_providers,
		vec![CiProviderKind::GitHubActions, CiProviderKind::GitLabCi]
	);
	assert!(npm.registry_setup_verifiable);
	assert!(npm.registry_setup_automation);
	assert!(npm.registry_native_provenance);

	let crates = builtin_registry_trust_capabilities(RegistryKind::CratesIo);
	assert_eq!(
		crates.supported_providers,
		vec![CiProviderKind::GitHubActions]
	);
	assert!(!crates.registry_setup_verifiable);
	assert!(!crates.registry_native_provenance);

	let pypi = builtin_registry_trust_capabilities(RegistryKind::Pypi);
	assert_eq!(
		pypi.supported_providers,
		vec![
			CiProviderKind::GitHubActions,
			CiProviderKind::GitLabCi,
			CiProviderKind::GoogleCloudBuild,
		]
	);
	assert!(!pypi.registry_setup_verifiable);
	assert!(pypi.registry_native_provenance);
}

#[test]
fn supported_provider_registry_combinations_are_claimed_explicitly() {
	let expected = [
		(RegistryKind::Npm, CiProviderKind::GitHubActions, true, true),
		(RegistryKind::Npm, CiProviderKind::GitLabCi, false, true),
		(
			RegistryKind::CratesIo,
			CiProviderKind::GitHubActions,
			false,
			false,
		),
		(
			RegistryKind::Jsr,
			CiProviderKind::GitHubActions,
			false,
			true,
		),
		(
			RegistryKind::PubDev,
			CiProviderKind::GitHubActions,
			false,
			false,
		),
		(
			RegistryKind::PubDev,
			CiProviderKind::GoogleCloudBuild,
			false,
			false,
		),
		(
			RegistryKind::Pypi,
			CiProviderKind::GitHubActions,
			false,
			true,
		),
		(RegistryKind::Pypi, CiProviderKind::GitLabCi, false, true),
		(
			RegistryKind::Pypi,
			CiProviderKind::GoogleCloudBuild,
			false,
			true,
		),
	];

	for (registry, provider, setup_verifiable, provenance) in expected {
		let capability = builtin_provider_registry_trust_capability(registry, provider);
		assert!(
			capability.trusted_publishing,
			"expected {provider:?} to be supported for {registry}"
		);
		assert!(capability.ci_identity_verifiable);
		assert_eq!(capability.registry_setup_verifiable, setup_verifiable);
		assert_eq!(capability.registry_setup_automation, setup_verifiable);
		assert_eq!(capability.registry_native_provenance, provenance);
	}
}

#[test]
fn provider_registry_capability_distinguishes_trust_from_provenance() {
	let npm_github = builtin_provider_registry_trust_capability(
		RegistryKind::Npm,
		CiProviderKind::GitHubActions,
	);
	assert!(npm_github.trusted_publishing);
	assert!(npm_github.registry_setup_verifiable);
	assert!(npm_github.registry_native_provenance);

	let crates_github = builtin_provider_registry_trust_capability(
		RegistryKind::CratesIo,
		CiProviderKind::GitHubActions,
	);
	assert!(crates_github.trusted_publishing);
	assert!(!crates_github.registry_native_provenance);

	let jsr_circle =
		builtin_provider_registry_trust_capability(RegistryKind::Jsr, CiProviderKind::CircleCi);
	assert!(!jsr_circle.trusted_publishing);
	assert!(!jsr_circle.registry_native_provenance);
}

#[test]
fn custom_registry_is_not_treated_as_trusted_by_default() {
	let custom = provider_registry_trust_capability(
		&PublishRegistry::Custom("https://registry.example.com".to_string()),
		CiProviderKind::GitHubActions,
	);

	assert!(!custom.trusted_publishing);
	assert!(!custom.ci_identity_verifiable);
	assert!(!custom.registry_setup_verifiable);
	assert!(
		custom
			.notes
			.iter()
			.any(|note| note.contains("custom/private"))
	);
}

#[test]
fn diagnostics_report_unsupported_and_unknown_contexts() {
	let circle_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Npm),
		&TrustedPublishingIdentity::CircleCi {
			project_slug: Some("gh/monochange/monochange".to_string()),
			workflow_id: Some("workflow".to_string()),
			job_name: Some("publish".to_string()),
		},
	);
	assert!(circle_message.contains("CircleCI is not supported for npm trusted publishing"));
	assert!(circle_message.contains("GitHub Actions, GitLab CI/CD"));

	let unknown_identity = TrustedPublishingIdentity::Unknown {
		reason: "local shell".to_string(),
	};
	assert!(!unknown_identity.is_verifiable_by_env());
	let unknown_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Pypi),
		&unknown_identity,
	);
	assert!(unknown_message.contains("No supported CI provider identity"));
	assert!(unknown_message.contains("Google Cloud Build"));
}

#[test]
fn diagnostics_report_incomplete_and_supported_context_capabilities() {
	let incomplete_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Npm),
		&TrustedPublishingIdentity::GitHubActions {
			repository: None,
			workflow: Some("publish.yml".to_string()),
			workflow_ref: None,
			environment: None,
			ref_name: None,
			run_id: None,
		},
	);
	assert!(incomplete_message.contains("publish-time environment variables are incomplete"));

	let complete_github_identity = TrustedPublishingIdentity::GitHubActions {
		repository: Some("monochange/monochange".to_string()),
		workflow: Some("publish.yml".to_string()),
		workflow_ref: None,
		environment: Some("publisher".to_string()),
		ref_name: Some("main".to_string()),
		run_id: Some("123".to_string()),
	};
	let npm_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Npm),
		&complete_github_identity,
	);
	assert!(npm_message.contains("monochange can verify registry-side setup"));
	assert!(npm_message.contains("registry-native provenance is available"));

	let crates_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::CratesIo),
		&complete_github_identity,
	);
	assert!(crates_message.contains("registry-side setup verification is manual"));
	assert!(crates_message.contains("registry-native provenance is not available"));
}

#[test]
fn unsupported_builtin_registries_have_no_trusted_publishing_capabilities() {
	let goproxy = builtin_registry_trust_capabilities(RegistryKind::GoProxy);

	assert_eq!(goproxy.registry, "go_proxy");
	assert!(!goproxy.trusted_publishing);
	assert!(goproxy.supported_providers.is_empty());
	assert_eq!(goproxy.setup_url, None);
	assert_eq!(
		goproxy.notes,
		vec!["unknown registry capabilities are treated as unsupported".to_string()]
	);

	let message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::GoProxy),
		&TrustedPublishingIdentity::Unknown {
			reason: "local shell".to_string(),
		},
	);
	assert!(message.contains("supported providers: none"));
}

fn publication_target(package: &str, ecosystem: Ecosystem) -> PackagePublicationTarget {
	PackagePublicationTarget {
		package: package.to_string(),
		ecosystem,
		registry: None,
		version: "1.0.0".to_string(),
		mode: PublishMode::default(),
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
	}
}

fn group_definition(id: &str, packages: &[&str]) -> GroupDefinition {
	GroupDefinition {
		id: id.to_string(),
		packages: packages
			.iter()
			.map(|package| (*package).to_string())
			.collect(),
		package_max_bumps: BTreeMap::new(),
		changelog: None,
		changelog_include: GroupChangelogInclude::default(),
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: VersionFormat::default(),
	}
}

#[test]
fn select_release_publication_targets_filters_ecosystems_and_expands_groups() {
	let groups = vec![
		group_definition("frontend", &["web", "ui"]),
		group_definition("docs", &["site"]),
	];
	let publication_targets = vec![
		publication_target("web", Ecosystem::Npm),
		publication_target("cli", Ecosystem::Cargo),
		publication_target("docs", Ecosystem::Python),
	];
	let selected_packages = BTreeSet::from(["manual".to_string()]);
	let selected_groups = BTreeSet::from(["frontend".to_string(), "missing".to_string()]);
	let selected_ecosystems = BTreeSet::from([Ecosystem::Npm, Ecosystem::Cargo]);

	let selected = select_release_publication_targets(
		&groups,
		&publication_targets,
		&selected_packages,
		&selected_groups,
		&selected_ecosystems,
	);

	assert_eq!(selected.publication_targets.len(), 2);
	assert_eq!(selected.publication_targets[0].package, "web");
	assert_eq!(selected.publication_targets[1].package, "cli");
	assert_eq!(
		selected.selected_packages,
		BTreeSet::from(["manual".to_string(), "web".to_string(), "ui".to_string()])
	);
}

fn sample_publish_request_for_registry(registry: RegistryKind) -> PublishRequest {
	PublishRequest {
		package_id: "pkg".to_string(),
		package_name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from("package.json"),
		package_root: PathBuf::from("."),
		registry,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		placeholder_readme: "placeholder".to_string(),
	}
}

#[test]
fn render_command_error_preserves_available_output_streams() {
	let stdout_only = CommandOutput {
		success: false,
		stdout: "useful stdout".to_string(),
		stderr: String::new(),
	};
	assert_eq!(render_command_error(&stdout_only), "useful stdout");

	let stderr_only = CommandOutput {
		success: false,
		stdout: String::new(),
		stderr: "useful stderr".to_string(),
	};
	assert_eq!(render_command_error(&stderr_only), "useful stderr");

	let both = CommandOutput {
		success: false,
		stdout: "useful stdout".to_string(),
		stderr: "useful stderr".to_string(),
	};
	assert_eq!(
		render_command_error(&both),
		"stdout:\nuseful stdout\n\nstderr:\nuseful stderr"
	);

	let empty = CommandOutput {
		success: false,
		stdout: String::new(),
		stderr: String::new(),
	};
	assert_eq!(
		render_command_error(&empty),
		"command failed without output"
	);
}

#[test]
fn render_publish_command_error_adds_npm_otp_recovery_guidance() {
	let request = sample_publish_request_for_registry(RegistryKind::Npm);
	let output = CommandOutput {
		success: false,
		stdout: String::new(),
		stderr: "npm error code EOTP\nnpm error This operation requires a one-time password."
			.to_string(),
	};

	let message =
		render_publish_command_error(&output, &request, PackagePublishRunMode::Placeholder);

	assert!(message.contains("npm error code EOTP"));
	assert!(message.contains("monochange step placeholder-publish --otp <CODE>"));
	assert!(!message.contains("`mc "));
	assert!(message.contains("NPM_CONFIG_OTP=<CODE>"));

	let release_message =
		render_publish_command_error(&output, &request, PackagePublishRunMode::Release);
	assert!(release_message.contains("rerun the publish command with `NPM_CONFIG_OTP=<CODE>`"));
}

#[test]
fn render_publish_command_error_adds_pub_dev_trusted_publishing_guidance() {
	let mut request = sample_publish_request_for_registry(RegistryKind::PubDev);
	request.ecosystem = Ecosystem::Dart;
	let output = CommandOutput {
		success: false,
		stdout: "Pub needs your authorization to upload packages on your behalf.".to_string(),
		stderr: "No credentials were available for pub.dev authentication.".to_string(),
	};

	let message = render_publish_command_error(&output, &request, PackagePublishRunMode::Release);

	insta::assert_snapshot!("pub_dev_trusted_publishing_auth_error", message);
}

#[test]
fn publish_command_sets_stream_output_environment_when_metadata_is_enabled() {
	let mut request = sample_publish_request_for_registry(RegistryKind::Npm);
	request.package_metadata.insert(
		PUBLISH_STREAM_OUTPUT_METADATA_KEY.to_string(),
		"true".to_string(),
	);

	let command = build_publish_command(&request, PackagePublishRunMode::Release, None, false);

	assert_eq!(
		command
			.env
			.get(PUBLISH_STREAM_OUTPUT_ENV_KEY)
			.map(String::as_str),
		Some("true")
	);
}

#[test]
fn process_command_executor_streams_when_environment_is_enabled() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let mut command = CommandSpec {
		program: "sh".to_string(),
		args: vec![
			"-c".to_string(),
			"printf stdout; printf stderr >&2".to_string(),
		],
		cwd: root.path().to_path_buf(),
		env: BTreeMap::from([(
			PUBLISH_STREAM_OUTPUT_ENV_KEY.to_string(),
			"true".to_string(),
		)]),
		timeout: None,
	};
	let mut executor = ProcessCommandExecutor::new(false);

	let output = executor
		.run(&command)
		.unwrap_or_else(|error| panic!("run streaming command: {error}"));
	assert!(output.success);
	assert_eq!(output.stdout, "stdout");
	assert_eq!(output.stderr, "stderr");

	command.program = "definitely-not-a-real-command".to_string();
	let error = executor
		.run(&command)
		.expect_err("invalid streaming command should fail");
	assert!(error.to_string().contains("failed to run"));
}

#[test]
fn process_command_executor_closes_stdin_for_captured_commands() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let command = CommandSpec {
		program: "sh".to_string(),
		args: vec!["-c".to_string(), "read ignored".to_string()],
		cwd: root.path().to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	};
	let mut executor = ProcessCommandExecutor::new(false);

	let output = executor
		.run(&command)
		.unwrap_or_else(|error| panic!("run command: {error}"));
	assert!(!output.success);
}

#[test]
fn child_output_helpers_cover_success_and_error_paths() {
	let captured = tee_child_output("chunk".as_bytes(), Vec::new())
		.unwrap_or_else(|error| panic!("tee output: {error}"));
	assert_eq!(captured, b"chunk");

	let spec = CommandSpec {
		program: "sh".to_string(),
		args: Vec::new(),
		cwd: PathBuf::from("."),
		env: BTreeMap::new(),
		timeout: None,
	};
	let wait_io_error = std::io::Error::other("wait failed");
	let wait_error = process_command_error(&spec, "wait for", &wait_io_error);
	assert!(
		wait_error
			.to_string()
			.contains("failed to wait for `sh` in .: wait failed")
	);
	let panic_thread = std::thread::spawn(|| -> std::io::Result<Vec<u8>> {
		panic!("reader panic");
	});
	let panic_error = join_child_output(panic_thread, &spec, "stdout")
		.expect_err("panic should become an io error");
	assert!(panic_error.to_string().contains("output reader panicked"));

	let io_thread = std::thread::spawn(|| -> std::io::Result<Vec<u8>> {
		Err(std::io::Error::other("reader failed"))
	});
	let io_error =
		join_child_output(io_thread, &spec, "stderr").expect_err("io error should be reported");
	assert!(io_error.to_string().contains("reader failed"));
}

#[derive(Default)]
struct RecordingPublishProgressReporter {
	events: std::sync::Mutex<Vec<PublishProgressEvent>>,
}

impl PublishProgressReporter for RecordingPublishProgressReporter {
	fn report(&self, event: PublishProgressEvent) {
		self.events.lock().unwrap().push(event);
	}
}

struct PanickingCommandExecutor;

impl CommandExecutor for PanickingCommandExecutor {
	fn run(&mut self, _spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		panic!("external packages must not run publish commands");
	}
}

struct SequencedCommandExecutor {
	commands: Vec<CommandSpec>,
	outputs: std::collections::VecDeque<MonochangeResult<CommandOutput>>,
}

impl SequencedCommandExecutor {
	fn new(outputs: impl IntoIterator<Item = MonochangeResult<CommandOutput>>) -> Self {
		Self {
			commands: Vec::new(),
			outputs: outputs.into_iter().collect(),
		}
	}
}

impl CommandExecutor for SequencedCommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		self.commands.push(spec.clone());
		self.outputs
			.pop_front()
			.unwrap_or_else(|| panic!("unexpected publish command: {}", render_command(spec)))
	}
}

fn publish_request(package: &str) -> PublishRequest {
	let mut request = sample_publish_request_for_registry(RegistryKind::Npm);
	request.package_id = package.to_string();
	request.package_name = package.to_string();
	request
}

fn npm_registry_response_endpoints(
	request_count: usize,
	response: &'static [u8],
) -> (RegistryEndpoints, std::thread::JoinHandle<()>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind test registry: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("test registry address: {error}"));
	let thread = std::thread::spawn(move || {
		for _ in 0..request_count {
			let (mut stream, _) = listener
				.accept()
				.unwrap_or_else(|error| panic!("accept registry request: {error}"));
			let mut request = [0_u8; 2048];
			std::io::Read::read(&mut stream, &mut request)
				.unwrap_or_else(|error| panic!("read registry request: {error}"));
			std::io::Write::write_all(&mut stream, response)
				.unwrap_or_else(|error| panic!("write registry response: {error}"));
		}
	});
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.npm_registry = format!("http://{address}");
	(endpoints, thread)
}

fn npm_not_found_endpoints(
	request_count: usize,
) -> (RegistryEndpoints, std::thread::JoinHandle<()>) {
	npm_registry_response_endpoints(
		request_count,
		b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
	)
}

fn npm_failure_endpoints() -> (RegistryEndpoints, std::thread::JoinHandle<()>) {
	npm_registry_response_endpoints(
		1,
		b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
	)
}

fn command_output(success: bool, stdout: &str, stderr: &str) -> CommandOutput {
	CommandOutput {
		success,
		stdout: stdout.to_string(),
		stderr: stderr.to_string(),
	}
}

fn assert_complete_failed_publish_run(
	report: &PackagePublishReport,
	requests: &[PublishRequest],
	progress: &RecordingPublishProgressReporter,
	expected_failure: &str,
	expected_placeholder: bool,
) {
	assert_eq!(report.packages.len(), requests.len());
	assert_eq!(
		report
			.packages
			.iter()
			.map(|outcome| outcome.package.as_str())
			.collect::<Vec<_>>(),
		requests
			.iter()
			.map(|request| request.package_id.as_str())
			.collect::<Vec<_>>()
	);
	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(report.packages[0].message.contains(expected_failure));
	assert_eq!(report.packages[0].placeholder, expected_placeholder);
	assert!(report.packages[1..].iter().all(|outcome| {
		outcome.status == PackagePublishStatus::Blocked
			&& outcome.placeholder == expected_placeholder
			&& outcome.message.contains(&format!(
				"publishing {} {} failed earlier",
				requests[0].package_name, requests[0].version
			))
	}));
	assert_eq!(
		report.summary(),
		PackagePublishSummary {
			expected: requests.len(),
			succeeded: 0,
			failed: 1,
			skipped: requests.len() - 1,
		}
	);

	let events = progress.events.lock().unwrap();
	assert!(matches!(
		events.first(),
		Some(PublishProgressEvent::RunStarted {
			mode,
			dry_run,
			total,
			..
		}) if *mode == report.mode && *dry_run == report.dry_run && *total == requests.len()
	));
	assert!(events.iter().any(|event| {
		matches!(
			event,
			PublishProgressEvent::PackageFailed { package, message }
				if package.package_id == requests[0].package_id
					&& message.contains(expected_failure)
		)
	}));
	for request in &requests[1..] {
		assert!(events.iter().any(|event| {
			matches!(
				event,
				PublishProgressEvent::PackageSkipped { package, message }
					if package.package_id == request.package_id
						&& message.contains("failed earlier")
			)
		}));
	}
	assert!(matches!(
		events.last(),
		Some(PublishProgressEvent::RunFinished {
			mode,
			total,
			published: 0,
			skipped,
			failed: 1,
		}) if *mode == report.mode
			&& *total == requests.len()
			&& *skipped == requests.len() - 1
	));
}

struct TestPublishTrustHandler;

impl PublishTrustHandler for TestPublishTrustHandler {
	fn trust_outcome_for_skip(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		disabled_trust_outcome()
	}

	fn planned_trust_outcome(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		disabled_trust_outcome()
	}

	fn enforce_release_trust_prerequisites(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<()> {
		Ok(())
	}
}

struct FailingPublishTrustHandler;

impl PublishTrustHandler for FailingPublishTrustHandler {
	fn trust_outcome_for_skip(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		disabled_trust_outcome()
	}

	fn planned_trust_outcome(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		disabled_trust_outcome()
	}

	fn enforce_release_trust_prerequisites(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<()> {
		Err(MonochangeError::Config(
			"trusted publishing prerequisite failed".to_string(),
		))
	}
}

#[test]
fn ecosystem_progress_presentation_uses_portable_emojis() {
	assert_eq!(Ecosystem::Cargo.progress_emoji(), "🦀");
	assert_eq!(Ecosystem::Npm.progress_emoji(), "📦");
	assert_eq!(Ecosystem::Deno.progress_emoji(), "🦕");
	assert_eq!(Ecosystem::Dart.progress_emoji(), "🎯");
	assert_eq!(Ecosystem::Python.progress_emoji(), "🐍");
	assert_eq!(Ecosystem::Go.progress_emoji(), "🐹");
	assert_eq!(Ecosystem::Cargo.progress_label(), "cargo");
	assert_eq!(progress_emoji_for_label("future"), "🌐");
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_uses_noop_progress_reporter_by_default() {
	let report = execute_publish_requests_with_process(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[],
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
	)
	.await
	.unwrap();

	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_with_process_and_progress_uses_noop_reporter() {
	let report = execute_publish_requests_with_process_and_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[],
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&NoopPublishProgressReporter,
	)
	.await
	.unwrap();

	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_progress_reports_external_skip_and_summary_events() {
	let mut request = sample_publish_request_for_registry(RegistryKind::Npm);
	request.mode = PublishMode::External;
	let requests = vec![request];
	let progress = RecordingPublishProgressReporter::default();
	let mut executor = PanickingCommandExecutor;

	let report = execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&RegistryEndpoints::from_env(),
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.unwrap();

	assert_eq!(
		report.packages[0].status,
		PackagePublishStatus::SkippedExternal
	);
	let events = progress.events.lock().unwrap();
	assert!(matches!(
		events.first(),
		Some(PublishProgressEvent::RunStarted { total: 1, ecosystems, .. })
			if ecosystems == &vec![Ecosystem::Npm]
	));
	assert!(matches!(
		&events[1],
		PublishProgressEvent::PackageSkipped { package, message }
			if package.package_name == "pkg" && message == "package opted out of built-in publishing"
	));
	assert!(matches!(
		events.last(),
		Some(PublishProgressEvent::RunFinished {
			total: 1,
			published: 0,
			skipped: 1,
			failed: 0,
			..
		})
	));
}

#[tokio::test(flavor = "multi_thread")]
async fn real_publish_failure_records_tail_outcomes_and_progress_summary() {
	let requests = [
		publish_request("first"),
		publish_request("failed"),
		publish_request("tail"),
	];
	let (endpoints, registry_thread) = npm_not_found_endpoints(2);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor = SequencedCommandExecutor::new([
		Ok(command_output(true, "published first", "")),
		Ok(command_output(
			false,
			"partial upload",
			"registry rejected package",
		)),
	]);

	let report = execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.unwrap_or_else(|error| panic!("execute publish requests: {error}"));
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert_eq!(executor.commands.len(), 2);
	assert_eq!(report.packages.len(), requests.len());
	assert_eq!(
		report
			.packages
			.iter()
			.map(|outcome| outcome.status)
			.collect::<Vec<_>>(),
		vec![
			PackagePublishStatus::Published,
			PackagePublishStatus::Failed,
			PackagePublishStatus::Blocked,
		]
	);
	assert_eq!(
		report.summary(),
		PackagePublishSummary {
			expected: 3,
			succeeded: 1,
			failed: 1,
			skipped: 1,
		}
	);

	assert!(report.packages[1].message.contains("partial upload"));
	assert!(
		report.packages[1]
			.message
			.contains("registry rejected package")
	);
	assert!(
		report.packages[2]
			.message
			.contains("publishing failed 1.0.0 failed earlier")
	);

	let events = progress.events.lock().unwrap();
	assert!(events.iter().any(|event| {
		matches!(
			event,
			PublishProgressEvent::PackageSkipped { package, message }
				if package.package_name == "tail" && message.contains("failed earlier")
		)
	}));
	assert!(matches!(
		events.last(),
		Some(PublishProgressEvent::RunFinished {
			total: 3,
			published: 1,
			skipped: 1,
			failed: 1,
			..
		})
	));
}

#[tokio::test(flavor = "multi_thread")]
async fn real_publish_spawn_failure_records_every_unattempted_package() {
	let requests = [
		publish_request("failed"),
		publish_request("tail-one"),
		publish_request("tail-two"),
	];
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let mut executor = SequencedCommandExecutor::new([Err(MonochangeError::Io(
		"publisher executable was not found".to_string(),
	))]);

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&NoopPublishProgressReporter,
	)
	.await
	.unwrap_or_else(|error| panic!("execute publish requests: {error}"));
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert_eq!(executor.commands.len(), 1);
	assert_eq!(report.packages.len(), 3);
	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(
		report.packages[1..]
			.iter()
			.all(|outcome| outcome.status == PackagePublishStatus::Blocked)
	);
	assert_eq!(
		report.summary(),
		PackagePublishSummary {
			expected: 3,
			succeeded: 0,
			failed: 1,
			skipped: 2,
		}
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_lookup_failure_records_failed_package_blocked_tail_and_finished_total() {
	let requests = [
		publish_request("failed"),
		publish_request("tail-one"),
		publish_request("tail-two"),
	];
	let (endpoints, registry_thread) = npm_failure_endpoints();
	let progress = RecordingPublishProgressReporter::default();
	let mut executor =
		SequencedCommandExecutor::new(std::iter::empty::<MonochangeResult<CommandOutput>>());

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("registry lookup failure should carry a report")
	.into_report();
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert!(executor.commands.is_empty());
	assert_complete_failed_publish_run(&report, &requests, &progress, "npm registry lookup", false);
}

#[tokio::test(flavor = "multi_thread")]
async fn readiness_checker_error_records_failed_package_blocked_tail_and_finished_total() {
	let requests = [publish_request("failed"), publish_request("tail")];
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let readiness = PublishReadinessRegistry::new().with_checker(
		RegistryKind::Npm,
		Box::new(|_, _| {
			Err(MonochangeError::Config(
				"readiness checker failed".to_string(),
			))
		}),
	);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor =
		SequencedCommandExecutor::new(std::iter::empty::<MonochangeResult<CommandOutput>>());

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&readiness,
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("readiness checker failure should carry a report")
	.into_report();
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert!(executor.commands.is_empty());
	assert_complete_failed_publish_run(
		&report,
		&requests,
		&progress,
		"readiness checker failed",
		false,
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn real_run_readiness_block_records_failed_package_instead_of_returning_error() {
	let requests = [publish_request("failed"), publish_request("tail")];
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let readiness = PublishReadinessRegistry::new().with_checker(
		RegistryKind::Npm,
		Box::new(|_, _| Ok(Some("release is not ready".to_string()))),
	);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor =
		SequencedCommandExecutor::new(std::iter::empty::<MonochangeResult<CommandOutput>>());

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&readiness,
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("blocked readiness should carry a report")
	.into_report();
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert!(executor.commands.is_empty());
	assert_complete_failed_publish_run(
		&report,
		&requests,
		&progress,
		"release is not ready",
		false,
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn placeholder_manifest_writer_failure_records_failed_package_and_blocked_tail() {
	let mut requests = [publish_request("failed"), publish_request("tail")];
	for request in &mut requests {
		request.placeholder = true;
	}
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let manifest_writers = PlaceholderManifestWriterRegistry::new().with_writer(
		RegistryKind::Npm,
		Box::new(|_, _, _, _| {
			Err(MonochangeError::Io(
				"placeholder manifest writer failed".to_string(),
			))
		}),
	);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor =
		SequencedCommandExecutor::new(std::iter::empty::<MonochangeResult<CommandOutput>>());

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Placeholder,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&manifest_writers,
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("placeholder manifest failure should carry a report")
	.into_report();
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert!(executor.commands.is_empty());
	assert_complete_failed_publish_run(
		&report,
		&requests,
		&progress,
		"placeholder manifest writer failed",
		true,
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn trust_prerequisite_failure_records_failed_package_and_blocked_tail() {
	let requests = [publish_request("failed"), publish_request("tail")];
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor =
		SequencedCommandExecutor::new(std::iter::empty::<MonochangeResult<CommandOutput>>());

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&FailingPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("trust prerequisite failure should carry a report")
	.into_report();
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert!(executor.commands.is_empty());
	assert_complete_failed_publish_run(
		&report,
		&requests,
		&progress,
		"trusted publishing prerequisite failed",
		false,
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn attestation_prerequisite_failure_records_failed_package_and_blocked_tail() {
	let mut requests = [publish_request("failed"), publish_request("tail")];
	requests[0].trusted_publishing.enabled = false;
	requests[0].attestations.require_registry_provenance = true;
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor =
		SequencedCommandExecutor::new(std::iter::empty::<MonochangeResult<CommandOutput>>());

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("attestation prerequisite failure should carry a report")
	.into_report();
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert!(executor.commands.is_empty());
	assert_complete_failed_publish_run(
		&report,
		&requests,
		&progress,
		"trusted publishing is disabled",
		false,
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_spawn_failure_records_failed_package_blocked_tail_and_expected_total() {
	let requests = [
		publish_request("failed"),
		publish_request("tail-one"),
		publish_request("tail-two"),
	];
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor = SequencedCommandExecutor::new([Err(MonochangeError::Io(
		"dry-run publisher executable was not found".to_string(),
	))]);

	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		true,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.unwrap_or_else(|error| panic!("execute publish dry run: {error}"));
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert_eq!(executor.commands.len(), 1);
	assert_complete_failed_publish_run(
		&report,
		&requests,
		&progress,
		"dry-run publisher executable was not found",
		false,
	);
}

#[test]
fn blocked_packages_are_pending_when_resuming() {
	let requests = [publish_request("done"), publish_request("tail")];
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			PackagePublishOutcome {
				package: "done".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: RegistryKind::Npm.to_string(),
				version: "1.0.0".to_string(),
				status: PackagePublishStatus::Published,
				message: "published".to_string(),
				placeholder: false,
				trusted_publishing: disabled_trust_outcome(),
				command: None,
				stdout: None,
				stderr: None,
			},
			PackagePublishOutcome {
				package: "tail".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: RegistryKind::Npm.to_string(),
				version: "1.0.0".to_string(),
				status: PackagePublishStatus::Blocked,
				message: "not attempted".to_string(),
				placeholder: false,
				trusted_publishing: disabled_trust_outcome(),
				command: None,
				stdout: None,
				stderr: None,
			},
		],
	};

	assert!(!package_publish_status_is_resumable_complete(
		PackagePublishStatus::Blocked
	));
	let (pending, completed) = resume_publish_requests(&requests, Some(&report))
		.unwrap_or_else(|error| panic!("resume requests: {error}"));
	assert_eq!(
		pending
			.iter()
			.map(|request| request.package_id.as_str())
			.collect::<Vec<_>>(),
		vec!["tail"]
	);
	assert_eq!(completed.len(), 1);
	assert_eq!(completed[0].package, "done");
}

#[test]
fn ensure_publish_report_succeeded_includes_summary_and_failed_package_detail() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			PackagePublishOutcome {
				package: "failed".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: RegistryKind::Npm.to_string(),
				version: "2.0.0".to_string(),
				status: PackagePublishStatus::Failed,
				message: "registry denied publication".to_string(),
				placeholder: false,
				trusted_publishing: disabled_trust_outcome(),
				command: None,
				stdout: None,
				stderr: None,
			},
			PackagePublishOutcome {
				package: "tail".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: RegistryKind::Npm.to_string(),
				version: "2.0.0".to_string(),
				status: PackagePublishStatus::Blocked,
				message: "not attempted".to_string(),
				placeholder: false,
				trusted_publishing: disabled_trust_outcome(),
				command: None,
				stdout: None,
				stderr: None,
			},
		],
	};

	let error = ensure_publish_report_succeeded(&report)
		.expect_err("failed report should produce an aggregate error")
		.to_string();
	assert!(error.contains("expected 2, succeeded 0, failed 1, skipped 1"));
	assert!(error.contains("failed package failed 2.0.0"));
	assert!(error.contains("registry denied publication"));
}

#[tokio::test(flavor = "multi_thread")]
async fn placeholder_publish_does_not_skip_external_mode_packages() {
	let mut request = sample_publish_request_for_registry(RegistryKind::Npm);
	request.mode = PublishMode::External;
	request.placeholder = true;
	let requests = vec![request];
	// In Placeholder mode, External packages should NOT be skipped —
	// placeholder publishing is a bootstrap utility, not normal release publishing.
	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Placeholder,
		false,
		&requests,
		&registry_client().unwrap(),
		&RegistryEndpoints::from_env(),
		&BTreeMap::new(),
		&mut PanickingCommandExecutor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
	)
	.await
	.unwrap();

	// The External package should NOT be skipped in Placeholder mode, so the
	// report should not contain SkippedExternal. Instead it will fail because
	// there is no real npm registry to check against, but it should have attempted
	// the publish (i.e., not been skipped at the mode gate).
	let skipped_external = report
		.packages
		.iter()
		.any(|p| p.status == PackagePublishStatus::SkippedExternal);
	assert!(
		!skipped_external,
		"placeholder publish should not skip external-mode packages, \
		 but found SkippedExternal in report"
	);
}

#[test]
fn publish_readiness_registry_push_checker_and_missing_checker_paths() {
	let request = sample_publish_request_for_registry(RegistryKind::Npm);
	let root = Path::new(".");
	let mut registry = PublishReadinessRegistry::new();

	assert_eq!(registry.blocked_message(root, &request).unwrap(), None);

	registry.push_checker(
		RegistryKind::Npm,
		Box::new(|_, request| Ok(Some(format!("{} blocked", request.package_name)))),
	);

	assert_eq!(
		registry.blocked_message(root, &request).unwrap().as_deref(),
		Some("pkg blocked")
	);
}

#[test]
fn placeholder_manifest_registry_push_writer_and_directory_builder_write_files() {
	let request = sample_publish_request_for_registry(RegistryKind::Npm);
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(root.path().join("LICENSE"), "source license")
		.unwrap_or_else(|error| panic!("write source license: {error}"));
	let mut registry = PlaceholderManifestWriterRegistry::new();

	registry.push_writer(
		RegistryKind::Npm,
		Box::new(|placeholder_dir, request, _, _| {
			fs::write(
				placeholder_dir.join("package.json"),
				format!("{{\"name\":\"{}\"}}", request.package_name),
			)
			.map_err(|error| MonochangeError::Io(error.to_string()))
		}),
	);

	let tempdir = build_placeholder_directory(root.path(), &request, None, &registry).unwrap();

	assert_eq!(
		fs::read_to_string(tempdir.path().join("README.md")).unwrap(),
		"placeholder"
	);
	assert_eq!(
		fs::read_to_string(tempdir.path().join("LICENSE")).unwrap(),
		"source license"
	);
	assert_eq!(
		fs::read_to_string(tempdir.path().join("CHANGELOG.md")).unwrap(),
		"## 1.0.0\n\n- Placeholder release.\n"
	);
	assert_eq!(
		fs::read_to_string(tempdir.path().join("package.json")).unwrap(),
		"{\"name\":\"pkg\"}"
	);

	let root_without_license =
		tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fallback_tempdir =
		build_placeholder_directory(root_without_license.path(), &request, None, &registry)
			.unwrap();
	assert_eq!(
		fs::read_to_string(fallback_tempdir.path().join("LICENSE")).unwrap(),
		"Placeholder package published by monochange. See the source repository for license terms.\n"
	);

	let placeholder_file =
		tempfile::NamedTempFile::new().unwrap_or_else(|error| panic!("placeholder file: {error}"));
	let error = write_placeholder_license(root_without_license.path(), placeholder_file.path())
		.expect_err("expected placeholder license write error");
	assert!(
		error
			.to_string()
			.contains("failed to write placeholder LICENSE")
	);
}

#[test]
fn default_registry_kind_for_ecosystem_reports_unknown_and_known_ecosystems() {
	let unknown = default_registry_kind_for_ecosystem("unknown").unwrap_err();
	assert!(unknown.to_string().contains("ecosystem `unknown`"));

	assert_eq!(
		default_registry_kind_for_ecosystem("go").unwrap(),
		RegistryKind::GoProxy
	);
}

#[test]
fn placeholder_tempdir_error_includes_io_error() {
	let error = std::io::Error::other("no tempdir");

	assert!(
		placeholder_tempdir_error(&error)
			.to_string()
			.contains("failed to create placeholder tempdir: no tempdir")
	);
}

#[test]
fn publish_dependency_order_handles_realistic_cargo_dependency_graph() {
	let schema = publish_order_package("schema");

	let mut codegen = publish_order_package("codegen");
	codegen
		.declared_dependencies
		.push(publish_order_dependency("schema", DependencyKind::Runtime));

	let mut test_helpers = publish_order_package("test_helpers");
	test_helpers
		.declared_dependencies
		.push(publish_order_dependency("schema", DependencyKind::Runtime));

	let mut core = publish_order_package("core");
	core.declared_dependencies
		.push(publish_order_dependency("schema", DependencyKind::Build));
	core.declared_dependencies.push(publish_order_dependency(
		"test_helpers",
		DependencyKind::Development,
	));

	let mut cli = publish_order_package("cli");
	cli.declared_dependencies
		.push(publish_order_dependency("core", DependencyKind::Runtime));
	cli.declared_dependencies
		.push(publish_order_dependency("codegen", DependencyKind::Build));
	cli.declared_dependencies.push(publish_order_dependency(
		"test_helpers",
		DependencyKind::Development,
	));

	let ordered = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&[cli, core, test_helpers, codegen, schema],
		vec![
			publish_order_request("cli"),
			publish_order_request("core"),
			publish_order_request("test_helpers"),
			publish_order_request("codegen"),
			publish_order_request("schema"),
		],
	)
	.unwrap_or_else(|error| panic!("publish requests should be ordered: {error}"));
	let ordered_package_ids = ordered
		.iter()
		.map(|request| request.package_id.as_str())
		.collect::<Vec<_>>();

	assert_eq!(
		ordered_package_ids,
		vec!["schema", "codegen", "test_helpers", "core", "cli"]
	);
}

#[test]
fn publish_dependency_order_reports_development_dependency_cycles() {
	let mut app = publish_order_package("app");
	app.declared_dependencies.push(publish_order_dependency(
		"helper",
		DependencyKind::Development,
	));
	let mut helper = publish_order_package("helper");
	helper
		.declared_dependencies
		.push(publish_order_dependency("app", DependencyKind::Development));

	let error = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&[app, helper],
		vec![
			publish_order_request("app"),
			publish_order_request("helper"),
		],
	)
	.expect_err("development dependency cycle should be rejected");

	assert!(
		error
			.to_string()
			.contains("cyclic publish dependencies detected")
	);
}

#[test]
fn npm_publish_order_ignores_peer_dependencies_by_default() {
	let peer = npm_publish_order_package("peer", Vec::new());
	let app = npm_publish_order_package(
		"app",
		vec![npm_publish_order_dependency("peer", "peerDependencies")],
	);
	let packages = vec![app.clone(), peer.clone()];
	let requests = vec![
		publish_order_request_for_package(&app),
		publish_order_request_for_package(&peer),
	];

	let ordered = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&packages,
		requests,
	)
	.unwrap();

	let package_names = ordered
		.into_iter()
		.map(|request| request.package_name)
		.collect::<Vec<_>>();
	assert_eq!(package_names, ["app", "peer"]);
}

#[test]
fn npm_publish_order_can_include_peer_dependencies() {
	let peer = npm_publish_order_package("peer", Vec::new());
	let app = npm_publish_order_package(
		"app",
		vec![npm_publish_order_dependency("peer", "peerDependencies")],
	);
	let packages = vec![app.clone(), peer.clone()];
	let requests = vec![
		publish_order_request_for_package(&app),
		publish_order_request_for_package(&peer),
	];

	let ordered = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(Some(vec![
			"dependencies",
			"devDependencies",
			"peerDependencies",
		])),
		&packages,
		requests,
	)
	.unwrap();

	let package_names = ordered
		.into_iter()
		.map(|request| request.package_name)
		.collect::<Vec<_>>();
	assert_eq!(package_names, ["peer", "app"]);
}

#[test]
fn npm_publish_order_can_include_custom_manifest_fields() {
	let tempdir = TempDir::new().unwrap();
	let workspace = tempdir.path();
	fs::create_dir_all(workspace.join("packages/app")).unwrap();
	fs::write(
		workspace.join("packages/app/package.json"),
		r#"{"name":"app","version":"1.0.0","catalogDependencies":{"external":"1.0.0","tool":"1.0.0"}}"#,
	)
	.unwrap();

	let tool = npm_publish_order_package("tool", Vec::new());
	let mut app = npm_publish_order_package("app", Vec::new());
	app.manifest_path = workspace.join("packages/app/package.json");
	let packages = vec![app.clone(), tool.clone()];
	let requests = vec![
		publish_order_request_for_package(&app),
		publish_order_request_for_package(&tool),
	];

	let ordered = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(Some(vec!["dependencies", "catalogDependencies"])),
		&packages,
		requests,
	)
	.unwrap();

	let package_names = ordered
		.into_iter()
		.map(|request| request.package_name)
		.collect::<Vec<_>>();
	assert_eq!(package_names, ["tool", "app"]);
}

#[test]
fn npm_publish_order_can_remove_dev_dependencies() {
	let tool = npm_publish_order_package("tool", Vec::new());
	let app = npm_publish_order_package(
		"app",
		vec![npm_publish_order_dependency("tool", "devDependencies")],
	);
	let packages = vec![app.clone(), tool.clone()];
	let requests = vec![
		publish_order_request_for_package(&app),
		publish_order_request_for_package(&tool),
	];

	let ordered = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(Some(vec!["dependencies"])),
		&packages,
		requests,
	)
	.unwrap();

	let package_names = ordered
		.into_iter()
		.map(|request| request.package_name)
		.collect::<Vec<_>>();
	assert_eq!(package_names, ["app", "tool"]);
}

#[test]
fn non_npm_publish_order_uses_matching_ecosystem_defaults() {
	let tool = python_publish_order_package("tool", Vec::new());
	let app = python_publish_order_package(
		"app",
		vec![publish_order_dependency_from_field("tool", "dependencies")],
	);
	let packages = vec![app.clone(), tool.clone()];
	let requests = vec![
		publish_order_request_for_package(&app),
		publish_order_request_for_package(&tool),
	];

	let ordered = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&packages,
		requests,
	)
	.unwrap();

	let package_names = ordered
		.into_iter()
		.map(|request| request.package_name)
		.collect::<Vec<_>>();
	assert_eq!(package_names, ["tool", "app"]);
}

#[test]
fn python_publish_order_can_include_optional_dependencies() {
	let extra = python_publish_order_package("extra", Vec::new());
	let app = python_publish_order_package(
		"app",
		vec![publish_order_dependency_from_field(
			"extra",
			"optional-dependencies",
		)],
	);
	let packages = vec![app.clone(), extra.clone()];

	let default_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&extra),
		],
	)
	.unwrap();
	assert_eq!(publish_order_package_names(default_order), ["app", "extra"]);

	let configured_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration_for(
			Ecosystem::Python,
			vec!["dependencies", "optional-dependencies"],
		),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&extra),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(configured_order),
		["extra", "app"]
	);
}

#[test]
fn go_publish_order_can_disable_require_dependencies() {
	let library = ecosystem_publish_order_package(Ecosystem::Go, "library", Vec::new());
	let app = ecosystem_publish_order_package(
		Ecosystem::Go,
		"app",
		vec![publish_order_dependency_from_field("library", "require")],
	);
	let packages = vec![app.clone(), library.clone()];

	let default_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&library),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(default_order),
		["library", "app"]
	);

	let configured_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration_for(Ecosystem::Go, Vec::new()),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&library),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(configured_order),
		["app", "library"]
	);
}

#[test]
fn dart_publish_order_can_remove_dev_dependencies() {
	let test_tool = ecosystem_publish_order_package(Ecosystem::Dart, "test_tool", Vec::new());
	let app = ecosystem_publish_order_package(
		Ecosystem::Dart,
		"app",
		vec![publish_order_dependency_from_field(
			"test_tool",
			"dev_dependencies",
		)],
	);
	let packages = vec![app.clone(), test_tool.clone()];

	let default_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&test_tool),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(default_order),
		["test_tool", "app"]
	);

	let configured_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration_for(Ecosystem::Dart, vec!["dependencies"]),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&test_tool),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(configured_order),
		["app", "test_tool"]
	);
}

#[test]
fn deno_publish_order_can_remove_dependencies() {
	let shared = ecosystem_publish_order_package(Ecosystem::Deno, "shared", Vec::new());
	let app = ecosystem_publish_order_package(
		Ecosystem::Deno,
		"app",
		vec![publish_order_dependency_from_field(
			"shared",
			"dependencies",
		)],
	);
	let packages = vec![app.clone(), shared.clone()];

	let default_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration(None),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&shared),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(default_order),
		["shared", "app"]
	);

	let configured_order = order_release_requests_by_publish_dependencies(
		&publish_order_configuration_for(Ecosystem::Deno, vec!["imports"]),
		&packages,
		vec![
			publish_order_request_for_package(&app),
			publish_order_request_for_package(&shared),
		],
	)
	.unwrap();
	assert_eq!(
		publish_order_package_names(configured_order),
		["app", "shared"]
	);
}

fn publish_order_package(name: &str) -> PackageRecord {
	let root = PathBuf::from("/workspace");
	let mut package = PackageRecord::new(
		Ecosystem::Cargo,
		name,
		root.join(name).join("Cargo.toml"),
		root,
		None,
		PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), name.to_string());
	package
}

fn publish_order_dependency(name: &str, kind: DependencyKind) -> PackageDependency {
	PackageDependency {
		name: name.to_string(),
		kind,
		version_constraint: Some("1.0.0".to_string()),
		optional: false,
		source_field: Some(
			match kind {
				DependencyKind::Development => "dev-dependencies",
				DependencyKind::Build => "build-dependencies",
				_ => "dependencies",
			}
			.to_string(),
		),
	}
}

fn npm_publish_order_package(name: &str, dependencies: Vec<PackageDependency>) -> PackageRecord {
	let root = PathBuf::from("/workspace");
	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		name,
		root.join("packages").join(name).join("package.json"),
		root,
		None,
		PublishState::Public,
	);
	package.declared_dependencies = dependencies;
	package
}

fn python_publish_order_package(name: &str, dependencies: Vec<PackageDependency>) -> PackageRecord {
	ecosystem_publish_order_package(Ecosystem::Python, name, dependencies)
}

fn ecosystem_publish_order_package(
	ecosystem: Ecosystem,
	name: &str,
	dependencies: Vec<PackageDependency>,
) -> PackageRecord {
	let root = PathBuf::from("/workspace");
	let manifest_name = match ecosystem {
		Ecosystem::Cargo => "Cargo.toml",
		Ecosystem::Dart => "pubspec.yaml",
		Ecosystem::Python => "pyproject.toml",
		Ecosystem::Go => "go.mod",
		_ => "package.json",
	};
	let mut package = PackageRecord::new(
		ecosystem,
		name,
		root.join("packages").join(name).join(manifest_name),
		root,
		None,
		PublishState::Public,
	);
	package.declared_dependencies = dependencies;
	package
}

fn npm_publish_order_dependency(name: &str, source_field: &str) -> PackageDependency {
	publish_order_dependency_from_field(name, source_field)
}

fn publish_order_dependency_from_field(name: &str, source_field: &str) -> PackageDependency {
	PackageDependency {
		name: name.to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("1.0.0".to_string()),
		optional: false,
		source_field: Some(source_field.to_string()),
	}
}

fn publish_order_configuration(npm_dependency_fields: Option<Vec<&str>>) -> WorkspaceConfiguration {
	let npm = EcosystemSettings {
		publish_order: PublishOrderSettings {
			dependency_fields: npm_dependency_fields
				.map(|fields| fields.into_iter().map(str::to_string).collect()),
		},
		..EcosystemSettings::default()
	};

	WorkspaceConfiguration {
		root_path: PathBuf::from("/workspace"),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm,
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	}
}

fn publish_order_configuration_for(
	ecosystem: Ecosystem,
	dependency_fields: Vec<&str>,
) -> WorkspaceConfiguration {
	let mut configuration = publish_order_configuration(None);
	let settings = match ecosystem {
		Ecosystem::Cargo => &mut configuration.cargo,
		Ecosystem::Deno => &mut configuration.deno,
		Ecosystem::Dart => &mut configuration.dart,
		Ecosystem::Python => &mut configuration.python,
		Ecosystem::Go => &mut configuration.go,
		_ => &mut configuration.npm,
	};
	settings.publish_order.dependency_fields =
		Some(dependency_fields.into_iter().map(str::to_string).collect());
	configuration
}

fn publish_order_package_names(requests: Vec<PublishRequest>) -> Vec<String> {
	requests
		.into_iter()
		.map(|request| request.package_name)
		.collect()
}

fn publish_order_request_for_package(package: &PackageRecord) -> PublishRequest {
	PublishRequest {
		package_id: package.name.clone(),
		package_name: package.name.clone(),
		ecosystem: package.ecosystem,
		manifest_path: package.manifest_path.clone(),
		package_root: package.manifest_path.parent().unwrap().to_path_buf(),
		registry: if package.ecosystem == Ecosystem::Cargo {
			RegistryKind::CratesIo
		} else {
			RegistryKind::Npm
		},
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		placeholder_readme: String::new(),
	}
}

fn publish_order_request(package: &str) -> PublishRequest {
	PublishRequest {
		package_id: package.to_string(),
		package_name: package.to_string(),
		ecosystem: Ecosystem::Cargo,
		manifest_path: PathBuf::from(format!("/workspace/{package}/Cargo.toml")),
		package_root: PathBuf::from(format!("/workspace/{package}")),
		registry: RegistryKind::CratesIo,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		placeholder_readme: String::new(),
	}
}

#[derive(Debug, Default)]
struct RecordingExecutor {
	commands: Vec<CommandSpec>,
}

impl CommandExecutor for RecordingExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		self.commands.push(spec.clone());
		Ok(CommandOutput {
			success: true,
			stdout: "dry run ok".to_string(),
			stderr: "validated package".to_string(),
		})
	}
}

#[derive(Debug, Default)]
struct TestTrustHandler;

impl PublishTrustHandler for TestTrustHandler {
	fn trust_outcome_for_skip(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		disabled_trust_outcome()
	}

	fn planned_trust_outcome(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		disabled_trust_outcome()
	}

	fn enforce_release_trust_prerequisites(
		&self,
		_request: &PublishRequest,
		_source: Option<&SourceConfiguration>,
		_root: &Path,
		_env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<()> {
		Ok(())
	}
}

fn cargo_publish_request() -> PublishRequest {
	PublishRequest {
		package_id: "pkg".to_string(),
		package_name: "pkg".to_string(),
		ecosystem: Ecosystem::Cargo,
		manifest_path: PathBuf::from("crates/pkg/Cargo.toml"),
		package_root: PathBuf::from("crates/pkg"),
		registry: RegistryKind::CratesIo,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.2.3".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings {
			enabled: false,
			..TrustedPublishingSettings::default()
		},
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		placeholder_readme: String::new(),
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_publish_executes_registry_dry_run_and_captures_output() {
	let request = cargo_publish_request();
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let endpoints = RegistryEndpoints::from_env();
	let command_builder = build_publish_command_builder();
	let manifest_writers = PlaceholderManifestWriterRegistry::default();
	let readiness = PublishReadinessRegistry::default();
	let trust_handler = TestTrustHandler;
	let mut executor = RecordingExecutor::default();

	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		true,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
		&command_builder,
		&manifest_writers,
		&readiness,
		&trust_handler,
	)
	.await
	.unwrap_or_else(|error| panic!("execute publish dry run: {error}"));

	assert_eq!(executor.commands.len(), 1);
	assert!(
		executor.commands[0]
			.args
			.iter()
			.any(|arg| arg == "--dry-run")
	);
	let outcome = report
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected publish outcome"));
	assert_eq!(outcome.status, PackagePublishStatus::Planned);
	assert_eq!(outcome.stdout.as_deref(), Some("dry run ok"));
	assert_eq!(outcome.stderr.as_deref(), Some("validated package"));
	assert!(
		outcome
			.command
			.as_deref()
			.is_some_and(|command| command.contains("--dry-run"))
	);
}

#[test]
fn package_publish_failure_exposes_error_and_report_accessors() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: Vec::new(),
	};
	let error = MonochangeError::Config("boom".to_string());
	let failure = PackagePublishFailure::new(error, report.clone());

	assert_eq!(failure.report(), &report);
	assert_eq!(failure.error().to_string(), "config error: boom");
	assert_eq!(failure.render(), "config error: boom");
	assert_eq!(failure.to_string(), "config error: boom");
	assert!(std::error::Error::source(&failure).is_none());

	let into_report = failure.into_report();
	assert_eq!(into_report, report);

	let failure =
		PackagePublishFailure::new(MonochangeError::Config("boom".to_string()), report.clone());
	let (returned_error, returned_report) = failure.into_parts();
	assert_eq!(returned_error.to_string(), "config error: boom");
	assert_eq!(returned_report.mode, PackagePublishRunMode::Release);

	let failure = PackagePublishFailure::new(MonochangeError::Config("boom".to_string()), report);
	assert_eq!(failure.into_error().to_string(), "config error: boom");
}

#[test]
fn empty_publish_report_marks_every_request_as_blocked() {
	let requests = [publish_request("first"), publish_request("second")];
	let report = empty_publish_report(PackagePublishRunMode::Release, false, &requests);
	assert_eq!(report.mode, PackagePublishRunMode::Release);
	assert!(!report.dry_run);
	assert_eq!(report.packages.len(), 2);
	assert!(report.packages.iter().all(|outcome| {
		outcome.status == PackagePublishStatus::Blocked
			&& outcome.message.contains("could not start")
	}));
	assert_eq!(report.summary().expected, 2);
	assert_eq!(report.summary().failed, 0);
	assert_eq!(report.summary().skipped, 2);
}

fn timeout_error() -> MonochangeError {
	MonochangeError::Io(format!(
		"{PUBLISH_TIMEOUT_ERROR_PREFIX}: `dart pub publish` timed out after 60 seconds"
	))
}

fn pub_dev_publish_request(package: &str) -> PublishRequest {
	let mut request = publish_request(package);
	request.registry = RegistryKind::PubDev;
	request.ecosystem = Ecosystem::Dart;
	request
}

#[test]
fn is_publish_timeout_error_detects_timeout_marker() {
	assert!(is_publish_timeout_error(&timeout_error()));
	assert!(!is_publish_timeout_error(&MonochangeError::Config(
		"boom".to_string()
	)));
}

#[test]
fn run_publish_command_with_retries_retries_until_success() {
	let request = publish_request("pkg");
	let mut executor = SequencedCommandExecutor::new([
		Err(timeout_error()),
		Err(timeout_error()),
		Ok(CommandOutput {
			success: true,
			stdout: "published".to_string(),
			stderr: String::new(),
		}),
	]);
	let spec = CommandSpec {
		program: "dart".to_string(),
		args: vec!["pub".to_string(), "publish".to_string()],
		cwd: PathBuf::from("."),
		env: BTreeMap::new(),
		timeout: Some(Duration::from_secs(60)),
	};
	let output = run_publish_command_with_retries(&mut executor, &spec, &request)
		.expect("retry loop should succeed on the third attempt");
	assert!(output.success);
	assert_eq!(executor.commands.len(), 3);
}

#[test]
fn run_publish_command_with_retries_returns_timeout_after_exhausting_retries() {
	let mut request = publish_request("pkg");
	request.timeout.retries = 1;
	let mut executor = SequencedCommandExecutor::new([Err(timeout_error()), Err(timeout_error())]);
	let spec = CommandSpec {
		program: "dart".to_string(),
		args: vec!["pub".to_string(), "publish".to_string()],
		cwd: PathBuf::from("."),
		env: BTreeMap::new(),
		timeout: Some(Duration::from_secs(60)),
	};
	let error = run_publish_command_with_retries(&mut executor, &spec, &request)
		.expect_err("exhausted retries should surface the timeout error");
	assert!(is_publish_timeout_error(&error));
	assert_eq!(executor.commands.len(), 2);
}

#[test]
fn run_publish_command_with_retries_does_not_retry_non_timeout_errors() {
	let request = publish_request("pkg");
	let mut executor = SequencedCommandExecutor::new([
		Err(MonochangeError::Io(
			"publisher executable was not found".to_string(),
		)),
		Ok(CommandOutput {
			success: true,
			stdout: String::new(),
			stderr: String::new(),
		}),
	]);
	let spec = CommandSpec {
		program: "dart".to_string(),
		args: vec!["pub".to_string(), "publish".to_string()],
		cwd: PathBuf::from("."),
		env: BTreeMap::new(),
		timeout: Some(Duration::from_secs(60)),
	};
	let error = run_publish_command_with_retries(&mut executor, &spec, &request)
		.expect_err("non-timeout errors should not be retried");
	assert!(!is_publish_timeout_error(&error));
	assert_eq!(executor.commands.len(), 1);
}

#[test]
fn publish_command_failure_message_includes_dart_guidance_for_timeout() {
	let mut request = pub_dev_publish_request("pkg");
	request.timeout.retries = 2;
	let message = publish_command_failure_message(&request, &timeout_error());
	assert!(message.contains("timed out after 3 attempt(s)"));
	assert!(message.contains("pub.dev protected publishing"));
	assert!(message.contains("workflow_dispatch"));
}

#[test]
fn publish_command_failure_message_omits_dart_guidance_for_non_timeout_errors() {
	let request = pub_dev_publish_request("pkg");
	let message = publish_command_failure_message(
		&request,
		&MonochangeError::Config("publish rejected".to_string()),
	);
	assert_eq!(message, "config error: publish rejected");
}

#[test]
fn dart_protected_publishing_warning_warns_for_workflow_dispatch_without_pub_token() {
	let request = pub_dev_publish_request("pkg");
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_EVENT_NAME".to_string(),
			"workflow_dispatch".to_string(),
		),
		("GITHUB_REF".to_string(), "refs/heads/main".to_string()),
	]);
	let warning = dart_protected_publishing_warning(&request, &env_map)
		.expect("workflow_dispatch without PUB_TOKEN should warn");
	assert!(warning.contains("workflow_dispatch"));
	assert!(warning.contains("PUB_TOKEN"));
}

#[test]
fn dart_protected_publishing_warning_returns_none_for_tag_push() {
	let request = pub_dev_publish_request("pkg");
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_EVENT_NAME".to_string(), "push".to_string()),
		("GITHUB_REF".to_string(), "refs/tags/v1.0.0".to_string()),
	]);
	assert!(dart_protected_publishing_warning(&request, &env_map).is_none());
}

#[test]
fn dart_protected_publishing_warning_returns_none_when_pub_token_present() {
	let request = pub_dev_publish_request("pkg");
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_EVENT_NAME".to_string(),
			"workflow_dispatch".to_string(),
		),
		("GITHUB_REF".to_string(), "refs/heads/main".to_string()),
		("PUB_TOKEN".to_string(), "secret".to_string()),
	]);
	assert!(dart_protected_publishing_warning(&request, &env_map).is_none());
}

#[test]
fn dart_protected_publishing_warning_returns_none_for_non_dart_registries() {
	let request = publish_request("pkg");
	let env_map = BTreeMap::from([
		(
			"GITHUB_EVENT_NAME".to_string(),
			"workflow_dispatch".to_string(),
		),
		("GITHUB_REF".to_string(), "refs/heads/main".to_string()),
	]);
	assert!(dart_protected_publishing_warning(&request, &env_map).is_none());
}

#[test]
fn process_command_executor_kills_command_exceeding_timeout() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let spec = CommandSpec {
		program: "sh".to_string(),
		args: vec!["-c".to_string(), "sleep 5".to_string()],
		cwd: root.path().to_path_buf(),
		env: BTreeMap::new(),
		timeout: Some(Duration::from_millis(300)),
	};
	let mut executor = ProcessCommandExecutor::new(false);
	let error = executor
		.run(&spec)
		.expect_err("sleep command should be killed by the timeout");
	assert!(
		is_publish_timeout_error(&error),
		"expected timeout error, got: {error}"
	);
	assert!(error.to_string().contains("timed out after"));
}

#[test]
fn process_command_executor_allows_command_within_timeout() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let spec = CommandSpec {
		program: "sh".to_string(),
		args: vec!["-c".to_string(), "printf done".to_string()],
		cwd: root.path().to_path_buf(),
		env: BTreeMap::new(),
		timeout: Some(Duration::from_secs(5)),
	};
	let mut executor = ProcessCommandExecutor::new(false);
	let output = executor
		.run(&spec)
		.unwrap_or_else(|error| panic!("run command within timeout: {error}"));
	assert!(output.success);
	assert_eq!(output.stdout, "done");
}

#[test]
fn publish_command_timeout_uses_request_settings() {
	let mut request = publish_request("pkg");
	request.timeout.timeout_seconds = 90;
	assert_eq!(
		publish_command_timeout(&request),
		Some(Duration::from_secs(90))
	);
	request.timeout.timeout_seconds = 0;
	assert_eq!(publish_command_timeout(&request), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn dart_protected_publishing_warning_emitted_for_workflow_dispatch_publish() {
	let request = pub_dev_publish_request("pkg");
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind test registry: {error}"));
	let registry_address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("registry address: {error}"));
	let registry_thread = std::thread::spawn(move || {
		let Ok((mut stream, _)) = listener.accept() else {
			return;
		};
		let mut request = [0_u8; 2048];
		let _ = std::io::Read::read(&mut stream, &mut request);
		let _ = std::io::Write::write_all(
			&mut stream,
			b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
		);
	});
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_EVENT_NAME".to_string(),
			"workflow_dispatch".to_string(),
		),
		("GITHUB_REF".to_string(), "refs/heads/main".to_string()),
	]);
	let mut executor = SequencedCommandExecutor::new([Ok(CommandOutput {
		success: true,
		stdout: "published pkg".to_string(),
		stderr: String::new(),
	})]);
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.pub_dev_api = format!("http://{registry_address}");
	let report = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		std::slice::from_ref(&request),
		&registry_client().unwrap(),
		&endpoints,
		&env_map,
		&mut executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&NoopPublishProgressReporter,
	)
	.await
	.unwrap_or_else(|error| panic!("publish request: {error}"));
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));
	assert_eq!(report.packages.len(), 1);
	assert_eq!(executor.commands.len(), 1);
	assert!(executor.commands[0].program.contains("dart"));
}
