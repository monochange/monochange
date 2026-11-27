#![allow(clippy::disallowed_methods)]

use std::io::Write as _;
use std::sync::Arc;
use std::sync::Mutex;

use monochange_core::ChangelogSettings;
use monochange_core::ChangesetSettings;
use monochange_core::PackageDependency;
use monochange_core::PublishOrderSettings;
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
		swift: EcosystemSettings::default(),
		cocoapods: EcosystemSettings::default(),
		gradle: EcosystemSettings::default(),
		version_schemes: BTreeMap::new(),
	}
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
		flow: PublishFlow::Direct,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: "placeholder".to_string(),
	}
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
	events: Mutex<Vec<PublishProgressEvent>>,
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
			planned: 0,
			published: 0,
			staged: 0,
			already_exists: 0,
			blocked: requests.len() - 1,
			failed: 1,
			not_attempted: 0,
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
			staged: 0,
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

fn npm_published_endpoints(
	request_count: usize,
) -> (RegistryEndpoints, std::thread::JoinHandle<()>) {
	npm_registry_response_endpoints(
		request_count,
		b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"versions\":{\"1.0.0\":{}}}",
	)
}

fn duplicate_publish_request(package: &str, fail_on_duplicate: bool) -> PublishRequest {
	let mut request = publish_request(package);
	request.fail_on_duplicate = fail_on_duplicate;
	request
}

#[tokio::test(flavor = "multi_thread")]
async fn release_run_fails_duplicate_versions_when_fail_on_duplicate_is_enabled() {
	let requests = [
		duplicate_publish_request("first", true),
		duplicate_publish_request("tail", false),
	];
	let (endpoints, registry_thread) = npm_published_endpoints(1);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor = SequencedCommandExecutor::new([]);

	let failure = try_execute_publish_requests_with_progress(
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
	.await;
	let Err(failure) = failure else {
		panic!("expected duplicate publish failure, got a successful report")
	};
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));

	assert_eq!(executor.commands.len(), 0);
	let (error, report) = failure.into_parts();
	assert!(error.to_string().contains("publish.fail_on_duplicate"));
	assert_eq!(
		report
			.packages
			.iter()
			.map(|outcome| (outcome.package.as_str(), outcome.status))
			.collect::<Vec<_>>(),
		vec![
			("first", PackagePublishStatus::Failed),
			("tail", PackagePublishStatus::Blocked)
		]
	);
	assert_eq!(
		report.summary(),
		PackagePublishSummary {
			planned: 0,
			published: 0,
			staged: 0,
			already_exists: 0,
			blocked: 1,
			failed: 1,
			not_attempted: 0,
		}
	);
	assert!(
		report.packages[0]
			.message
			.contains("first 1.0.0 already exists on npm and `publish.fail_on_duplicate` rejects")
	);
	assert!(
		report.packages[1]
			.message
			.contains("publishing first 1.0.0 failed earlier")
	);
	let events = progress.events.lock().unwrap();

	assert!(events.iter().any(|event| {
		matches!(
			event,
			PublishProgressEvent::PackageFailed { package, message }
				if package.package_name == "first"
					&& message.contains("publish.fail_on_duplicate")
		)
	}));
	assert!(matches!(
		events.last(),
		Some(PublishProgressEvent::RunFinished {
			total: 2,
			published: 0,
			skipped: 1,
			failed: 1,
			..
		})
	));
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
				rate_limit: None,
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
				rate_limit: None,
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
fn ensure_publish_report_succeeded_explains_registry_rate_limits() {
	let outcome = |package: &str, status, rate_limit| {
		PackagePublishOutcome {
			package: package.to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: RegistryKind::CratesIo.to_string(),
			version: "0.0.0".to_string(),
			status,
			message: "outcome".to_string(),
			placeholder: true,
			trusted_publishing: disabled_trust_outcome(),
			command: None,
			stdout: None,
			stderr: None,
			rate_limit,
		}
	};
	let hit = PublishRateLimitHit {
		reason: Some(
			"You have published too many new crates in a short period of time.".to_string(),
		),
		retry_at: Some("2026-10-20T10:59:10+00:00".to_string()),
		docs_url: Some(CRATES_IO_RATE_LIMIT_DOCS.to_string()),
	};
	let mut report = PackagePublishReport {
		mode: PackagePublishRunMode::Placeholder,
		dry_run: false,
		packages: vec![
			outcome("canvist_canvas", PackagePublishStatus::Published, None),
			outcome(
				"canvist_editor",
				PackagePublishStatus::SkippedExisting,
				None,
			),
			outcome(
				"canvist_layout",
				PackagePublishStatus::Failed,
				Some(hit.clone()),
			),
			outcome("canvist_model", PackagePublishStatus::Blocked, None),
		],
	};
	let now = chrono::DateTime::parse_from_rfc3339("2026-10-20T10:53:00Z")
		.unwrap_or_else(|error| panic!("now: {error}"))
		.with_timezone(&chrono::Utc);

	let error = ensure_publish_report_succeeded_at(&report, now)
		.expect_err("rate-limited report should fail")
		.to_string();
	insta::assert_snapshot!("rate_limited_publish_error_with_retry_time", error);

	report.packages.truncate(3);
	report.packages[2].rate_limit = Some(PublishRateLimitHit {
		reason: None,
		retry_at: None,
		docs_url: None,
	});
	let error = ensure_publish_report_succeeded_at(&report, now)
		.expect_err("rate-limited report should fail")
		.to_string();
	insta::assert_snapshot!("rate_limited_publish_error_without_retry_time", error);
}

#[tokio::test(flavor = "multi_thread")]
async fn real_publish_rate_limit_reports_the_registry_reason_instead_of_the_tool_log() {
	let requests = [publish_request("limited"), publish_request("tail")];
	let (endpoints, registry_thread) = npm_not_found_endpoints(1);
	let progress = RecordingPublishProgressReporter::default();
	let mut executor = SequencedCommandExecutor::new([Ok(command_output(
		false,
		"   Packaging limited v1.0.0",
		"error: failed to publish limited\n\nCaused by:\n  the remote server responded with an \
		 error (status 429 Too Many Requests): You have published too many new crates in a short \
		 period of time. Please try again after Tue, 20 Oct 2020 10:59:10 GMT and see \
		 https://crates.io/docs/rate-limits for more details.",
	))]);

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

	let limited = &report.packages[0];
	assert_eq!(limited.status, PackagePublishStatus::Failed);
	assert_eq!(
		limited.message,
		"rate limited by npm until Tue 20 Oct 2020 10:59:10 UTC. You have published too many new \
		 crates in a short period of time."
	);
	assert_eq!(
		limited
			.rate_limit
			.as_ref()
			.and_then(|hit| hit.retry_at.as_deref()),
		Some("2020-10-20T10:59:10+00:00")
	);
	assert!(
		limited
			.stderr
			.as_deref()
			.is_some_and(|stderr| stderr.contains("Caused by:")),
		"the full tool output stays in the report"
	);
	assert_eq!(report.packages[1].status, PackagePublishStatus::Blocked);

	let events = progress.events.lock().unwrap();
	assert!(events.iter().any(|event| {
		matches!(
			event,
			PublishProgressEvent::PackageFailed { message, .. }
				if message.starts_with("rate limited by npm") && !message.contains('\n')
		)
	}));
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
		swift: EcosystemSettings::default(),
		cocoapods: EcosystemSettings::default(),
		gradle: EcosystemSettings::default(),
		version_schemes: BTreeMap::new(),
	}
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
		flow: PublishFlow::Direct,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: String::new(),
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
	assert_eq!(report.summary().total(), 2);
	assert_eq!(report.summary().failed, 0);

	assert_eq!(report.summary().blocked, 2);
}

fn pub_dev_publish_request(package: &str) -> PublishRequest {
	let mut request = publish_request(package);
	request.registry = RegistryKind::PubDev;
	request.ecosystem = Ecosystem::Dart;
	request
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

#[tokio::test(flavor = "multi_thread")]
async fn pub_dev_trusted_publishing_ref_error_fails_the_run_before_minting() {
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

	let failure = try_execute_publish_requests_with_progress(
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
	.expect_err("a branch-ref trusted publish must fail the run");
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));
	let report = failure.into_report();
	assert_eq!(report.packages.len(), 1);
	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(report.packages[0].message.contains("not a tag"));
	assert!(
		executor.commands.is_empty(),
		"no dart command may run once the run ref is known to be rejected, got: {:#?}",
		executor.commands
	);
}

fn trusted_pub_dev_publish_request(package: &str) -> PublishRequest {
	let mut request = pub_dev_publish_request(package);
	request.trusted_publishing = TrustedPublishingSettings {
		enabled: true,
		..TrustedPublishingSettings::default()
	};
	request
}

/// Serve `request_count` HTTP responses and capture the raw requests.
fn spawn_http_capture_server(
	request_count: usize,
	response: Vec<u8>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind http capture server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("http capture server address: {error}"));
	let captured = Arc::new(Mutex::new(Vec::new()));
	let captured_clone = Arc::clone(&captured);
	let handle = std::thread::spawn(move || {
		for _ in 0..request_count {
			let Ok((mut stream, _)) = listener.accept() else {
				break;
			};
			let mut request = Vec::new();
			let mut buffer = [0_u8; 2048];
			while let Ok(read) = std::io::Read::read(&mut stream, &mut buffer) {
				if read == 0 {
					break;
				}

				request.extend_from_slice(&buffer[..read]);
				if request.windows(4).any(|window| window == b"\r\n\r\n") {
					break;
				}
			}

			captured_clone
				.lock()
				.unwrap()
				.push(String::from_utf8_lossy(&request).into_owned());
			let _ = stream.write_all(&response);
		}
	});
	(format!("http://{address}"), captured, handle)
}

fn http_json_response(body: &str) -> Vec<u8> {
	format!(
		"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
		body.len()
	)
	.into_bytes()
}

fn http_server_error_response() -> Vec<u8> {
	b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
}

fn pub_dev_not_found_endpoints(
	request_count: usize,
) -> (
	RegistryEndpoints,
	std::thread::JoinHandle<()>,
	Arc<Mutex<Vec<String>>>,
) {
	let (base_url, captured, server) = spawn_http_capture_server(
		request_count,
		b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
	);
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.pub_dev_api = base_url;
	(endpoints, server, captured)
}

fn actions_oidc_env_map(base_url: &str, request_token: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			ACTIONS_ID_TOKEN_REQUEST_URL_ENV.to_string(),
			format!("{base_url}/token"),
		),
		(
			ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV.to_string(),
			request_token.to_string(),
		),
	])
}

struct FailingTokenAddExecutor;

impl CommandExecutor for FailingTokenAddExecutor {
	fn run(&mut self, _spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		Ok(command_output(
			false,
			"token store is not writable",
			"permission denied",
		))
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn trusted_pub_dev_token_refresh_fails_when_the_pub_token_store_cannot_be_registered() {
	let (oidc_base_url, oidc_requests, oidc_server) = spawn_http_capture_server(
		1,
		http_json_response(r#"{"count":1,"value":"fresh-actions-jwt"}"#),
	);
	let env_map = actions_oidc_env_map(&oidc_base_url, "runner-secret");
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let request = trusted_pub_dev_publish_request("pkg");
	let mut executor = FailingTokenAddExecutor;
	let mut publish_command =
		build_publish_command(&request, PackagePublishRunMode::Release, None, false);

	let error = refresh_pub_dev_trusted_publishing_token(
		&client,
		&env_map,
		&mut executor,
		&request,
		&mut publish_command,
	)
	.await
	.expect_err("a failed pub token registration must surface as an error");

	assert!(
		error
			.render()
			.contains("`dart pub token add https://pub.dev --env-var PUB_TOKEN` in . failed"),
		"unexpected error: {error}"
	);
	assert!(
		error
			.render()
			.contains("the pub token store must be writable"),
		"unexpected error: {error}"
	);
	assert_eq!(
		publish_command.env.get(PUB_TOKEN_ENV_VAR),
		Some(&"fresh-actions-jwt".to_string())
	);
	assert_eq!(oidc_requests.lock().unwrap().len(), 1);
	oidc_server
		.join()
		.unwrap_or_else(|_| panic!("oidc server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn dart_trusted_publish_fails_when_the_oidc_token_cannot_be_minted() {
	let requests = vec![
		trusted_pub_dev_publish_request("pkg"),
		trusted_pub_dev_publish_request("other"),
	];
	let (oidc_base_url, oidc_requests, oidc_server) =
		spawn_http_capture_server(1, http_server_error_response());
	let env_map = actions_oidc_env_map(&oidc_base_url, "runner-secret");
	let (endpoints, pub_dev_server, _captured) = pub_dev_not_found_endpoints(1);
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let command_builder = build_publish_command_builder();
	let manifest_writers = PlaceholderManifestWriterRegistry::default();

	let readiness = PublishReadinessRegistry::default();
	let trust_handler = TestTrustHandler;
	let progress = RecordingPublishProgressReporter::default();
	let mut executor = PanickingCommandExecutor;

	let failure = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&client,
		&endpoints,
		&env_map,
		&mut executor,
		&command_builder,
		&manifest_writers,
		&readiness,
		&trust_handler,
		&progress,
	)
	.await
	.expect_err("a failed OIDC mint must fail the publish run");

	assert_complete_failed_publish_run(
		failure.report(),
		&requests,
		&progress,
		"refreshing the pub.dev trusted publishing token",
		false,
	);
	assert_eq!(oidc_requests.lock().unwrap().len(), 1);
	oidc_server
		.join()
		.unwrap_or_else(|_| panic!("oidc server thread"));
	pub_dev_server
		.join()
		.unwrap_or_else(|_| panic!("pub.dev server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_preflight_blocks_before_any_publish_mutation() {
	let requests = [publish_request("first"), publish_request("blocked")];
	let (endpoints, registry_thread) = npm_not_found_endpoints(0);
	let progress = RecordingPublishProgressReporter::default();
	let readiness = PublishReadinessRegistry::new().with_checker(
		RegistryKind::Npm,
		Box::new(|_root, request| {
			if request.package_id == "blocked" {
				Ok(Some("blocked package cannot publish".to_string()))
			} else {
				Ok(None)
			}
		}),
	);

	let failure = try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&requests,
		&registry_client().unwrap(),
		&endpoints,
		&BTreeMap::new(),
		&mut PanickingCommandExecutor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&readiness,
		&TestPublishTrustHandler,
		&progress,
	)
	.await
	.expect_err("preflight blockers must fail the run before mutation");
	assert!(
		failure
			.error()
			.render()
			.contains("blocked package cannot publish")
	);
	assert!(
		failure.report().packages.is_empty(),
		"preflight must not record per-package outcomes"
	);
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("registry server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_package_exists_probes_crates_io_and_pub_dev_packages() {
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));

	let (base_url, _captured, server) = spawn_http_capture_server(
		1,
		http_json_response("{\"crate\":{\"id\":\"pkg\",\"max_version\":\"1.0.0\"}}"),
	);
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.crates_io_api = base_url;
	let crates_io_existing = registry_package_exists_with_transport(
		&sample_publish_request_for_registry(RegistryKind::CratesIo),
		&client,
		&endpoints,
	)
	.await
	.unwrap_or_else(|error| panic!("crates.io package lookup: {error}"));
	assert_eq!(crates_io_existing, Some(true));
	server
		.join()
		.unwrap_or_else(|_| panic!("crates.io server thread"));

	let (pub_dev_url, _pub_dev_captured, pub_dev_server) = spawn_http_capture_server(
		1,
		http_json_response("{\"versions\":[{\"version\":\"1.0.0\"}]}"),
	);
	let mut pub_dev_endpoints = RegistryEndpoints::from_env();
	pub_dev_endpoints.pub_dev_api = pub_dev_url;
	let pub_dev_existing = registry_package_exists_with_transport(
		&sample_publish_request_for_registry(RegistryKind::PubDev),
		&client,
		&pub_dev_endpoints,
	)
	.await
	.unwrap_or_else(|error| panic!("pub.dev package lookup: {error}"));
	assert_eq!(pub_dev_existing, Some(true));
	pub_dev_server
		.join()
		.unwrap_or_else(|_| panic!("pub.dev server thread"));
}

// -- cocoapods and gradle publishing --
fn cocoapods_publish_request(name: &str, version: &str) -> PublishRequest {
	let mut request = sample_publish_request_for_registry(RegistryKind::CocoaPodsTrunk);
	request.ecosystem = Ecosystem::CocoaPods;
	request.package_name = name.to_string();
	request.manifest_path = PathBuf::from(format!("{name}.podspec"));
	request.version = version.to_string();
	request
}

fn gradle_publish_request(coordinate: &str, version: &str) -> PublishRequest {
	let mut request = sample_publish_request_for_registry(RegistryKind::MavenCentral);
	request.ecosystem = Ecosystem::Gradle;
	request.package_name = coordinate.to_string();
	request.version = version.to_string();
	request
}

fn trunk_registry_response(body: &'static str) -> (RegistryEndpoints, std::thread::JoinHandle<()>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind trunk registry: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("trunk registry address: {error}"));
	let response = format!(
		"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
		body.len(),
		body
	);
	let thread = std::thread::spawn(move || {
		for _ in 0..2 {
			let Ok((mut stream, _)) = listener.accept() else {
				return;
			};
			let mut request = [0_u8; 2048];
			std::io::Read::read(&mut stream, &mut request)
				.unwrap_or_else(|error| panic!("read trunk request: {error}"));
			std::io::Write::write_all(&mut stream, response.as_bytes())
				.unwrap_or_else(|error| panic!("write trunk response: {error}"));
		}
	});
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.cocoapods_trunk_api = format!("http://{address}");
	(endpoints, thread)
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_version_exists_checks_the_cocoapods_trunk() {
	let (endpoints, thread) =
		trunk_registry_response(r#"{"versions":[{"name":"1.0.0"},{"name":"1.2.3"}]}"#);
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));

	let present = registry_version_exists(
		&client,
		&endpoints,
		&cocoapods_publish_request("Canvist", "1.2.3"),
	)
	.await
	.unwrap_or_else(|error| panic!("trunk lookup: {error}"));
	assert!(present);

	let missing = registry_version_exists(
		&client,
		&endpoints,
		&cocoapods_publish_request("Canvist", "2.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("trunk lookup: {error}"));
	assert!(!missing);

	thread
		.join()
		.unwrap_or_else(|_| panic!("trunk thread panicked"));
}

fn maven_registry_response(
	body: &'static str,
	status: &'static str,
	request_count: usize,
) -> (RegistryEndpoints, std::thread::JoinHandle<()>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind maven registry: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("maven registry address: {error}"));
	let response = format!(
		"HTTP/1.1 {status}\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
		body.len(),
		body
	);
	let thread = std::thread::spawn(move || {
		for _ in 0..request_count {
			let Ok((mut stream, _)) = listener.accept() else {
				return;
			};
			let mut request = [0_u8; 2048];
			std::io::Read::read(&mut stream, &mut request)
				.unwrap_or_else(|error| panic!("read maven request: {error}"));
			std::io::Write::write_all(&mut stream, response.as_bytes())
				.unwrap_or_else(|error| panic!("write maven response: {error}"));
		}
	});
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.maven_central = format!("http://{address}");
	(endpoints, thread)
}

#[test]
fn cocoapods_adapter_covers_every_publish_method_directly() {
	let builder = build_publish_command_builder();
	let adapter = builder
		.adapter_for_registry(RegistryKind::CocoaPodsTrunk)
		.expect("cocoapods adapter");
	let request = cocoapods_publish_request("Canvist", "1.2.3");

	assert!(
		adapter
			.build_placeholder_command(&request, Path::new("/tmp/placeholder"))
			.is_none()
	);

	let release = adapter
		.build_release_command(&request)
		.expect("release command");
	assert_eq!(release.cwd, Path::new("."));

	let mut args = vec![
		"trunk".to_string(),
		"push".to_string(),
		"Canvist.podspec".to_string(),
	];
	adapter.append_dry_run_args(&mut args);
	assert_eq!(
		args.iter().map(String::as_str).collect::<Vec<_>>(),
		vec!["spec", "lint", "Canvist.podspec"]
	);

	assert_eq!(
		adapter.registry_setup_url(),
		Some("https://guides.cocoapods.org/making/getting-setup-with-trunk.html")
	);
	assert!(!adapter.registry_notes().is_empty());
	assert!(!adapter.supports_provenance());
	assert!(adapter.supported_providers().is_empty());
}

#[test]
fn gradle_adapter_covers_every_publish_method_directly() {
	let builder = build_publish_command_builder();
	let adapter = builder
		.adapter_for_registry(RegistryKind::MavenCentral)
		.expect("gradle adapter");
	let request = gradle_publish_request("dev.canvist:canvist-android", "0.2.0");

	let release = adapter
		.build_release_command(&request)
		.expect("release command");
	assert_eq!(
		release.program,
		if cfg!(windows) {
			"gradlew.bat"
		} else {
			"./gradlew"
		}
	);

	let mut args = vec!["publishAndReleaseToMavenCentral".to_string()];
	adapter.append_dry_run_args(&mut args);
	assert_eq!(args, ["publishAndReleaseToMavenCentral", "--dry-run"]);

	assert_eq!(
		adapter.registry_setup_url(),
		Some("https://central.sonatype.org/publish/publish-portal/")
	);
	assert!(!adapter.registry_notes().is_empty());
	assert!(!adapter.supports_provenance());
	assert!(adapter.supported_providers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_version_exists_treats_any_published_maven_artifact_as_placeholder_present() {
	let (endpoints, thread) = maven_registry_response(
		"<metadata><versioning><versions><version>0.1.0</version></versions></versioning></metadata>",
		"200 OK",
		1,
	);
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let mut request = gradle_publish_request("dev.canvist:canvist-android", "0.0.0");
	request.placeholder = true;

	let exists = registry_version_exists(&client, &endpoints, &request)
		.await
		.unwrap_or_else(|error| panic!("maven lookup: {error}"));

	assert!(exists);
	thread
		.join()
		.unwrap_or_else(|_| panic!("maven thread panicked"));
}
