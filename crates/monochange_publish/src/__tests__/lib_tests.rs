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
		release_tag: None,
		run_scope: PublishRunScope::Publish,
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
			deferred: 0,
			out_of_scope: 0,
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
			deferred: 0,
			out_of_scope: 0,
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
				deferred_to: None,
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
				deferred_to: None,
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
			deferred_to: None,
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
		release_tag: None,
		run_scope: PublishRunScope::Publish,
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

/// A pub.dev trusted-publishing request released under `tag`.
fn tag_bound_request(package: &str, tag: Option<&str>) -> PublishRequest {
	let mut request = pub_dev_publish_request(package);
	request.release_tag = tag.map(|name| {
		PublishReleaseTag {
			name: name.to_string(),
		}
	});
	request
}

/// An npm request released under `tag`, which any run ref can publish.
fn ref_free_request(package: &str, tag: Option<&str>) -> PublishRequest {
	let mut request = publish_request(package);
	request.release_tag = tag.map(|name| {
		PublishReleaseTag {
			name: name.to_string(),
		}
	});
	request
}

/// The tags a release creates: `primary` and `tags`.
fn release_tags(primary: Option<&str>, tags: &[&str]) -> PublishReleaseTags {
	PublishReleaseTags {
		primary: primary.map(str::to_string),
		tags: tags
			.iter()
			.copied()
			.chain(primary)
			.map(str::to_string)
			.collect(),
	}
}

/// The variables GitHub Actions sets for a `workflow_dispatch` run on
/// `git_ref`.
fn github_actions_env(git_ref: &str) -> BTreeMap<String, String> {
	let ref_type = if git_ref.starts_with("refs/tags/") {
		"tag"
	} else {
		"branch"
	};
	BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_EVENT_NAME".to_string(),
			"workflow_dispatch".to_string(),
		),
		("GITHUB_REF".to_string(), git_ref.to_string()),
		("GITHUB_REF_TYPE".to_string(), ref_type.to_string()),
		("GITHUB_REPOSITORY".to_string(), "owner/repo".to_string()),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			format!("owner/repo/.github/workflows/publish.yml@{git_ref}"),
		),
	])
}

/// One word per scope, plus the reason and tag of a deferral.
fn scope_summary(scope: &PublishRunScope) -> String {
	match scope {
		PublishRunScope::Publish => "publish".to_string(),
		PublishRunScope::DeferredToTag { deferral, .. } => {
			format!(
				"deferred {} {}",
				serde_json::to_value(deferral.reason)
					.ok()
					.and_then(|value| value.as_str().map(str::to_string))
					.unwrap_or_default(),
				deferral.tag
			)
		}
		PublishRunScope::OutOfScope { .. } => "out_of_scope".to_string(),
		PublishRunScope::MissingReleaseTag => "missing_release_tag".to_string(),
	}
}

fn plan_summary(
	requests: &[PublishRequest],
	tags: &PublishReleaseTags,
	env_map: &BTreeMap<String, String>,
) -> Vec<String> {
	plan_publish_run_scopes(requests, tags, env_map)
		.iter()
		.map(scope_summary)
		.collect()
}

fn scope_message(scope: &PublishRunScope) -> &str {
	match scope {
		PublishRunScope::DeferredToTag { message, .. }
		| PublishRunScope::OutOfScope { message } => message,
		PublishRunScope::Publish | PublishRunScope::MissingReleaseTag => "",
	}
}

#[test]
fn plan_publish_run_scopes_publishes_everything_outside_github_actions() {
	let requests = [
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
		tag_bound_request("untagged_dart", None),
		ref_free_request("web", Some("v2.0.0")),
	];
	let env_map = BTreeMap::from([("GITHUB_REF".to_string(), "refs/tags/v2.0.0".to_string())]);

	assert_eq!(
		plan_publish_run_scopes(&requests, &release_tags(Some("v2.0.0"), &[]), &env_map),
		vec![PublishRunScope::Publish; 3]
	);
	assert_eq!(github_actions_run_ref(&env_map), None);
}

#[test]
fn plan_publish_run_scopes_defers_tag_bound_package_on_the_primary_tag() {
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
		ref_free_request("untagged_web", None),
	];
	let scopes = plan_publish_run_scopes(
		&requests,
		&release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0"]),
		&github_actions_env("refs/tags/v2.0.0"),
	);

	assert_eq!(
		scopes.iter().map(scope_summary).collect::<Vec<_>>(),
		["publish", "deferred wrong_ref dart_pkg/v1.0.0", "publish"]
	);
	assert_eq!(
		scope_message(&scopes[1]),
		"pub.dev only accepts dart_pkg 1.0.0 from a run on `refs/tags/dart_pkg/v1.0.0`, and this \
		 run is on `refs/tags/v2.0.0`; publish it with `gh workflow run publish.yml --repo \
		 owner/repo --ref dart_pkg/v1.0.0`"
	);
}

#[test]
fn plan_publish_run_scopes_publishes_only_the_tag_bound_packages_on_their_tag() {
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
		tag_bound_request("other_dart", Some("other_dart/v3.0.0")),
		ref_free_request("cli", None),
		ref_free_request("schema", Some("schema/v0.4.0")),
	];
	let scopes = plan_publish_run_scopes(
		&requests,
		&release_tags(
			Some("v2.0.0"),
			&["dart_pkg/v1.0.0", "other_dart/v3.0.0", "schema/v0.4.0"],
		),
		&github_actions_env("refs/tags/dart_pkg/v1.0.0"),
	);

	assert_eq!(
		scopes.iter().map(scope_summary).collect::<Vec<_>>(),
		[
			"out_of_scope",
			"publish",
			"out_of_scope",
			"out_of_scope",
			"out_of_scope"
		]
	);
	insta::assert_snapshot!(
		"out_of_scope_messages_name_the_publishing_run",
		scopes
			.iter()
			.map(scope_message)
			.filter(|message| !message.is_empty())
			.collect::<Vec<_>>()
			.join("\n")
	);
}

#[test]
fn plan_publish_run_scopes_defers_on_unrelated_tags_and_branches() {
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
	];
	let tags = release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0"]);

	assert_eq!(
		plan_summary(
			&requests,
			&tags,
			&github_actions_env("refs/tags/unrelated/v9.9.9")
		),
		["publish", "deferred wrong_ref dart_pkg/v1.0.0"]
	);

	let scopes = plan_publish_run_scopes(&requests, &tags, &github_actions_env("refs/heads/main"));
	assert_eq!(
		scopes.iter().map(scope_summary).collect::<Vec<_>>(),
		["publish", "deferred wrong_ref_type dart_pkg/v1.0.0"]
	);
	assert!(
		scope_message(&scopes[1]).contains("this run is on the branch `refs/heads/main`"),
		"{}",
		scope_message(&scopes[1])
	);
}

#[test]
fn plan_publish_run_scopes_publishes_a_pub_dev_package_that_owns_the_primary_tag() {
	let requests = [
		tag_bound_request("dart_pkg", Some("v2.0.0")),
		ref_free_request("web", Some("web/v1.1.0")),
		ref_free_request("cli", None),
	];
	let tags = release_tags(Some("v2.0.0"), &["web/v1.1.0"]);

	assert_eq!(
		plan_summary(&requests, &tags, &github_actions_env("refs/tags/v2.0.0")),
		["publish", "publish", "publish"]
	);

	// The run on `web`'s own tag leaves every package to the primary tag's
	// run, which publishes them all.
	let web_tag = plan_publish_run_scopes(
		&requests,
		&tags,
		&github_actions_env("refs/tags/web/v1.1.0"),
	);
	assert_eq!(
		web_tag.iter().map(scope_summary).collect::<Vec<_>>(),
		["out_of_scope", "out_of_scope", "out_of_scope"]
	);
	assert_eq!(
		scope_message(&web_tag[0]),
		"not published in this run: dart_pkg 1.0.0 publishes in the run on `refs/tags/v2.0.0`, \
		 the only run pub.dev accepts it from"
	);
	assert_eq!(
		scope_message(&web_tag[1]),
		"not published in this run: web 1.0.0 publishes in the run on `refs/tags/v2.0.0`, because \
		 a release with trusted pub.dev packages publishes each package from one of its tags"
	);

	// Branches stay ordinary runs.
	assert_eq!(
		plan_summary(&requests, &tags, &github_actions_env("refs/heads/main")),
		["deferred wrong_ref_type v2.0.0", "publish", "publish"]
	);
}

#[test]
fn plan_publish_run_scopes_scopes_a_mixed_group_tag_to_its_pub_dev_members() {
	let requests = [
		ref_free_request("cli", Some("v2.0.1")),
		ref_free_request("web", Some("sdk/v1.1.0")),
		tag_bound_request("dart_pkg", Some("sdk/v1.1.0")),
	];
	let tags = release_tags(Some("v2.0.1"), &["sdk/v1.1.0"]);
	let scopes = plan_publish_run_scopes(
		&requests,
		&tags,
		&github_actions_env("refs/tags/sdk/v1.1.0"),
	);

	assert_eq!(
		scopes.iter().map(scope_summary).collect::<Vec<_>>(),
		["out_of_scope", "out_of_scope", "publish"]
	);
	assert!(
		scope_message(&scopes[1]).contains("web 1.0.0 publishes in the run on `refs/tags/v2.0.1`"),
		"{}",
		scope_message(&scopes[1])
	);
	assert_eq!(
		plan_summary(&requests, &tags, &github_actions_env("refs/tags/v2.0.1")),
		["publish", "publish", "deferred wrong_ref sdk/v1.1.0"]
	);
}

#[test]
fn plan_publish_run_scopes_publishes_everything_on_the_release_only_tag() {
	let requests = [
		ref_free_request("web", Some("sdk/v1.1.0")),
		tag_bound_request("dart_pkg", Some("sdk/v1.1.0")),
	];

	assert_eq!(
		plan_summary(
			&requests,
			&release_tags(None, &["sdk/v1.1.0"]),
			&github_actions_env("refs/tags/sdk/v1.1.0")
		),
		["publish", "publish"]
	);
}

#[test]
fn plan_publish_run_scopes_binds_pub_dev_packages_without_a_pub_token() {
	let external = {
		let mut request = tag_bound_request("external_dart", None);
		request.mode = PublishMode::External;
		request
	};
	let untrusted = {
		let mut request = tag_bound_request("untrusted_dart", None);
		request.trusted_publishing.enabled = false;
		request
	};
	let tagged = tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0"));
	let tags = release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0"]);
	let env_map = github_actions_env("refs/tags/v2.0.0");

	assert_eq!(
		plan_summary(&[external, untrusted], &tags, &env_map),
		["publish", "publish"]
	);

	let mut token_env = env_map.clone();
	token_env.insert(PUB_TOKEN_ENV_VAR.to_string(), "token".to_string());
	assert!(!publish_is_tag_bound(&tagged, &token_env));
	assert_eq!(
		plan_summary(std::slice::from_ref(&tagged), &tags, &token_env),
		["publish"]
	);

	// GitHub Actions passes an unset secret as an empty value.
	let mut empty_token_env = env_map;
	empty_token_env.insert(PUB_TOKEN_ENV_VAR.to_string(), String::new());
	assert!(publish_is_tag_bound(&tagged, &empty_token_env));
	assert_eq!(
		plan_summary(&[tagged], &tags, &empty_token_env),
		["deferred wrong_ref dart_pkg/v1.0.0"]
	);
}

#[test]
fn plan_publish_run_scopes_defers_runs_pub_dev_rejects_by_event_or_ref() {
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
	];
	let tags = release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0"]);
	let on_tag = github_actions_env("refs/tags/dart_pkg/v1.0.0");
	let with = |name: &str, value: Option<&str>| {
		let mut env_map = on_tag.clone();
		match value {
			Some(value) => env_map.insert(name.to_string(), value.to_string()),
			None => env_map.remove(name),
		};
		env_map
	};

	let release_event = plan_publish_run_scopes(
		&requests,
		&tags,
		&with("GITHUB_EVENT_NAME", Some("release")),
	);
	assert_eq!(
		release_event.iter().map(scope_summary).collect::<Vec<_>>(),
		["out_of_scope", "deferred wrong_event dart_pkg/v1.0.0"]
	);
	insta::assert_snapshot!(
		"deferral_message_wrong_event",
		scope_message(&release_event[1])
	);

	assert_eq!(
		plan_summary(&requests, &tags, &with("GITHUB_EVENT_NAME", Some("push"))),
		["out_of_scope", "publish"]
	);
	assert_eq!(
		plan_summary(&requests, &tags, &with("GITHUB_EVENT_NAME", None)),
		["out_of_scope", "publish"],
		"an unknown event is not known to be rejected"
	);
	assert_eq!(
		plan_summary(&requests, &tags, &with("GITHUB_REF_TYPE", Some("branch"))),
		["publish", "deferred wrong_ref_type dart_pkg/v1.0.0"]
	);

	let unknown_ref = plan_publish_run_scopes(&requests, &tags, &with("GITHUB_REF", None));
	assert_eq!(
		unknown_ref.iter().map(scope_summary).collect::<Vec<_>>(),
		["publish", "deferred unknown_ref dart_pkg/v1.0.0"]
	);
	insta::assert_snapshot!(
		"deferral_message_unknown_ref",
		scope_message(&unknown_ref[1])
	);

	let mut workflow_ref_only = github_actions_env("refs/tags/v2.0.0");
	workflow_ref_only.remove("GITHUB_ACTIONS");
	assert_eq!(
		plan_summary(&requests, &tags, &workflow_ref_only),
		["publish", "deferred wrong_ref dart_pkg/v1.0.0"],
		"`GITHUB_WORKFLOW_REF` alone identifies a GitHub Actions run"
	);
}

#[test]
fn plan_publish_run_scopes_reports_tag_bound_packages_without_a_release_tag() {
	let requests = [
		tag_bound_request("untagged_dart", None),
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
	];
	let tags = release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0"]);

	for git_ref in ["refs/tags/v2.0.0", "refs/tags/dart_pkg/v1.0.0"] {
		assert_eq!(
			plan_publish_run_scopes(&requests, &tags, &github_actions_env(git_ref))[0],
			PublishRunScope::MissingReleaseTag,
			"{git_ref}"
		);
	}
}

#[test]
fn plan_publish_run_scopes_never_scopes_a_release_without_tag_bound_packages() {
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		ref_free_request("cli", Some("cli/v1.0.1")),
		{
			let mut request = tag_bound_request("token_dart", Some("token_dart/v0.2.1"));
			request.trusted_publishing.enabled = false;
			request
		},
	];
	let tags = release_tags(Some("v2.0.0"), &["cli/v1.0.1", "token_dart/v0.2.1"]);

	for git_ref in [
		"refs/heads/main",
		"refs/tags/v2.0.0",
		"refs/tags/cli/v1.0.1",
		"refs/tags/token_dart/v0.2.1",
		"refs/tags/unrelated/v9.9.9",
	] {
		assert_eq!(
			plan_publish_run_scopes(&requests, &tags, &github_actions_env(git_ref)),
			vec![PublishRunScope::Publish; 3],
			"{git_ref}"
		);
	}
}

#[test]
fn deferral_command_dispatches_the_workflow_pub_dev_trusts() {
	let mut request = tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0"));
	let tags = release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0"]);
	let deferred = |request: &PublishRequest, env_map: &BTreeMap<String, String>| {
		match &plan_publish_run_scopes(std::slice::from_ref(request), &tags, env_map)[0] {
			PublishRunScope::DeferredToTag { deferral, message } => {
				(deferral.command.clone(), message.clone())
			}
			other => panic!("expected a deferral, got {other:?}"),
		}
	};
	let mut env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_REF".to_string(), "refs/tags/v2.0.0".to_string()),
	]);

	// Unknown workflow: omit `command` rather than rendering `<workflow>`,
	// and explain why in the message (for wrong_ref, wrong_ref_type,
	// wrong_event, and out-of-scope tag-bound messages).
	let (command, message) = deferred(&request, &env_map);
	assert_eq!(command, None);
	assert!(!message.contains("<workflow>"), "{message}");
	assert!(
		message.contains("publish.trusted_publishing.workflow"),
		"{message}"
	);

	let mut branch_env = env_map.clone();
	branch_env.insert("GITHUB_REF".to_string(), "refs/heads/main".to_string());
	let (branch_command, branch_message) = deferred(&request, &branch_env);
	assert_eq!(branch_command, None);
	assert!(
		branch_message.contains("publish.trusted_publishing.workflow"),
		"{branch_message}"
	);

	let mut release_event_env = env_map.clone();
	release_event_env.insert(
		"GITHUB_REF".to_string(),
		"refs/tags/dart_pkg/v1.0.0".to_string(),
	);
	release_event_env.insert("GITHUB_EVENT_NAME".to_string(), "release".to_string());
	let (event_command, event_message) = deferred(&request, &release_event_env);
	assert_eq!(event_command, None);
	assert!(
		event_message.contains("publish.trusted_publishing.workflow"),
		"{event_message}"
	);

	let second_dart = tag_bound_request("other_dart", Some("other_dart/v3.0.0"));
	let mut other_tag_env = env_map.clone();
	other_tag_env.insert(
		"GITHUB_REF".to_string(),
		"refs/tags/dart_pkg/v1.0.0".to_string(),
	);
	let two_tags_with_primary =
		release_tags(Some("v2.0.0"), &["dart_pkg/v1.0.0", "other_dart/v3.0.0"]);
	let scopes_with_coord = plan_publish_run_scopes(
		&[request.clone(), second_dart.clone()],
		&two_tags_with_primary,
		&other_tag_env,
	);
	assert!(
		scope_message(&scopes_with_coord[1]).contains("publish.trusted_publishing.workflow")
			&& !scope_message(&scopes_with_coord[1]).contains("<workflow>"),
		"{}",
		scope_message(&scopes_with_coord[1])
	);
	let two_tags_without_primary = release_tags(None, &["dart_pkg/v1.0.0", "other_dart/v3.0.0"]);
	let scopes_without_coord = plan_publish_run_scopes(
		&[request.clone(), second_dart],
		&two_tags_without_primary,
		&other_tag_env,
	);
	assert!(
		scope_message(&scopes_without_coord[1]).contains("publish.trusted_publishing.workflow")
			&& !scope_message(&scopes_without_coord[1]).contains("<workflow>"),
		"{}",
		scope_message(&scopes_without_coord[1])
	);

	// Workflow display names containing spaces or quotes are shell-quoted so
	// `bash -c "$command"` cannot split or redirect them.
	env_map.insert(
		"GITHUB_WORKFLOW".to_string(),
		"Publish Packages".to_string(),
	);
	assert_eq!(
		deferred(&request, &env_map).0.as_deref(),
		Some("gh workflow run 'Publish Packages' --ref dart_pkg/v1.0.0")
	);

	env_map.insert("GITHUB_WORKFLOW".to_string(), "Bob's Publish".to_string());
	assert_eq!(
		deferred(&request, &env_map).0.as_deref(),
		Some("gh workflow run 'Bob'\\''s Publish' --ref dart_pkg/v1.0.0")
	);

	env_map.insert("GITHUB_WORKFLOW".to_string(), "publish".to_string());
	env_map.insert("GITHUB_REPOSITORY".to_string(), String::new());
	assert_eq!(
		deferred(&request, &env_map).0.as_deref(),
		Some("gh workflow run publish --ref dart_pkg/v1.0.0"),
		"an empty repository renders no `--repo` and clean names stay unquoted"
	);

	request.trusted_publishing.workflow = Some("publish-dart.yml".to_string());
	request.trusted_publishing.repository = Some("owner/dart".to_string());
	assert_eq!(
		deferred(&request, &github_actions_env("refs/tags/v2.0.0"))
			.0
			.as_deref(),
		Some("gh workflow run publish-dart.yml --repo owner/dart --ref dart_pkg/v1.0.0")
	);
}

/// How often each run of a release plans each package: one entry per
/// package, listing the release tags whose run plans it `Publish`.
fn publishing_runs(
	requests: &[PublishRequest],
	tags: &PublishReleaseTags,
) -> Vec<(String, Vec<String>)> {
	let plans = tags
		.tags
		.iter()
		.map(|tag| {
			(
				tag.clone(),
				plan_publish_run_scopes(
					requests,
					tags,
					&github_actions_env(&format!("refs/tags/{tag}")),
				),
			)
		})
		.collect::<Vec<_>>();

	requests
		.iter()
		.enumerate()
		.map(|(index, request)| {
			let runs = plans
				.iter()
				.filter(|(_, scopes)| scopes[index] == PublishRunScope::Publish)
				.map(|(tag, _)| tag.clone())
				.collect();
			(request.package_id.clone(), runs)
		})
		.collect()
}

/// The release tags whose run defers each package with a reason another run
/// picks up (`wrong_ref`), so the publish workflow dispatches it from there.
fn dispatching_runs(requests: &[PublishRequest], tags: &PublishReleaseTags) -> Vec<Vec<String>> {
	let mut runs = vec![Vec::new(); requests.len()];

	for tag in &tags.tags {
		let scopes = plan_publish_run_scopes(
			requests,
			tags,
			&github_actions_env(&format!("refs/tags/{tag}")),
		);

		for (index, scope) in scopes.iter().enumerate() {
			if let PublishRunScope::DeferredToTag { deferral, .. } = scope {
				assert_eq!(deferral.reason, PublishDeferralReason::WrongRef);
				runs[index].push(tag.clone());
			}
		}
	}

	runs
}

/// The layouts the publishing design names, as (layout, requests, tags).
fn named_release_layouts() -> Vec<(&'static str, Vec<PublishRequest>, PublishReleaseTags)> {
	vec![
		(
			"L1: a pub.dev package owns the primary tag beside npm and cargo",
			vec![
				tag_bound_request("app_dart", Some("v0.2.1")),
				ref_free_request("cli", Some("v0.2.1")),
				ref_free_request("web", Some("web/v1.1.0")),
			],
			release_tags(Some("v0.2.1"), &["web/v1.1.0"]),
		),
		(
			"L2: a group tag holds a pub.dev and an npm package; cargo owns the primary tag",
			vec![
				ref_free_request("cli", Some("v2.0.1")),
				tag_bound_request("app_dart", Some("sdk/v1.1.0")),
				ref_free_request("web", Some("sdk/v1.1.0")),
			],
			release_tags(Some("v2.0.1"), &["sdk/v1.1.0"]),
		),
		(
			"L3: no primary tag; a standalone npm package and pub.dev package",
			vec![
				ref_free_request("web", Some("web/v1.1.0")),
				tag_bound_request("app_dart", Some("app_dart/v0.2.1")),
			],
			release_tags(None, &["web/v1.1.0", "app_dart/v0.2.1"]),
		),
		(
			"two pub.dev tags, one shared with an npm package, and no other tag",
			vec![
				tag_bound_request("app_dart", Some("sdk/v1.1.0")),
				ref_free_request("web", Some("sdk/v1.1.0")),
				tag_bound_request("tools_dart", Some("tools_dart/v0.3.0")),
			],
			release_tags(None, &["sdk/v1.1.0", "tools_dart/v0.3.0"]),
		),
	]
}

/// Across the runs on every tag of the release, each package of the named
/// layouts is planned by exactly one run, and at most one run dispatches it.
#[test]
fn every_named_layout_publishes_each_package_in_exactly_one_tag_run() {
	let runs = named_release_layouts()
		.into_iter()
		.map(|(layout, requests, tags)| {
			let publishing = publishing_runs(&requests, &tags)
				.into_iter()
				.map(|(package, runs)| format!("{package}: {}", runs.join(", ")))
				.collect::<Vec<_>>()
				.join("; ");
			let dispatching = dispatching_runs(&requests, &tags)
				.into_iter()
				.zip(&requests)
				.filter(|(runs, _)| !runs.is_empty())
				.map(|(runs, request)| format!("{} from {}", request.package_id, runs.join(", ")))
				.collect::<Vec<_>>()
				.join("; ");
			format!("{layout}\n  publishes: {publishing}\n  dispatches: {dispatching}")
		})
		.collect::<Vec<_>>();

	assert_eq!(
		runs,
		[
			"L1: a pub.dev package owns the primary tag beside npm and cargo\n  publishes: \
			 app_dart: v0.2.1; cli: v0.2.1; web: v0.2.1\n  dispatches: ",
			"L2: a group tag holds a pub.dev and an npm package; cargo owns the primary tag\n  \
			 publishes: cli: v2.0.1; app_dart: sdk/v1.1.0; web: v2.0.1\n  dispatches: app_dart \
			 from v2.0.1",
			"L3: no primary tag; a standalone npm package and pub.dev package\n  publishes: web: \
			 web/v1.1.0; app_dart: app_dart/v0.2.1\n  dispatches: app_dart from web/v1.1.0",
			"two pub.dev tags, one shared with an npm package, and no other tag\n  publishes: \
			 app_dart: sdk/v1.1.0; web: sdk/v1.1.0; tools_dart: tools_dart/v0.3.0\n  dispatches: ",
		]
	);
}

/// Property: whatever packages and tags a release has, across the runs on
/// every tag of the release each package is planned by exactly one run, at
/// most one run (never a pub.dev package's own tag run) dispatches it, an
/// out-of-scope package names the run that plans it, the primary tag's run
/// publishes every package that is not tag-bound, and a release without
/// tag-bound packages is never scoped on any ref.
#[test]
fn plan_publish_run_scopes_plans_each_package_in_exactly_one_tag_run() {
	const PRIMARY: &str = "v2.0.0";
	let tag_choices = [None, Some(PRIMARY), Some("a/v1.0.0"), Some("b/v1.0.0")];
	let package_choices = tag_choices
		.iter()
		.flat_map(|tag| [(true, *tag), (false, *tag)])
		.collect::<Vec<_>>();
	let other_refs = ["refs/tags/x/v9.9.9", "refs/heads/main"];
	let mut checked = 0_usize;

	for first in &package_choices {
		for second in &package_choices {
			for third in &package_choices {
				let requests = [("p1", first), ("p2", second), ("p3", third)]
					.into_iter()
					.map(|(name, (bound, tag))| {
						if *bound {
							tag_bound_request(name, *tag)
						} else {
							ref_free_request(name, *tag)
						}
					})
					.collect::<Vec<_>>();

				for primary in [Some(PRIMARY), None] {
					for unpublished_tag in [None, Some("app/v1.0.0")] {
						let tags = PublishReleaseTags {
							primary: primary.map(str::to_string),
							tags: requests
								.iter()
								.filter_map(|request| request.release_tag.as_ref())
								.map(|tag| tag.name.clone())
								.chain(primary.map(str::to_string))
								.chain(unpublished_tag.map(str::to_string))
								.collect(),
						};
						let env_map = github_actions_env("refs/heads/main");
						let any_bound = requests
							.iter()
							.any(|request| publish_is_tag_bound(request, &env_map));
						let case = |request: &PublishRequest| {
							format!(
								"{} (tag {:?}) of [{first:?}, {second:?}, {third:?}], primary \
								 {primary:?}, unpublished {unpublished_tag:?}",
								request.package_id, request.release_tag
							)
						};
						let dispatching = dispatching_runs(&requests, &tags);
						// A run on one of these tags is itself dispatched; it never
						// dispatches again.
						let bound_tags = requests
							.iter()
							.filter(|request| publish_is_tag_bound(request, &env_map))
							.filter_map(|request| request.release_tag.as_ref())
							.map(|tag| tag.name.clone())
							.filter(|tag| Some(tag.as_str()) != primary)
							.collect::<BTreeSet<_>>();

						for ((request, (_, runs)), dispatched_from) in requests
							.iter()
							.zip(publishing_runs(&requests, &tags))
							.zip(&dispatching)
						{
							let missing_tag = publish_is_tag_bound(request, &env_map)
								&& request.release_tag.is_none();

							// A release without tag-bound packages publishes everything
							// on every ref, as before; checked below.
							if any_bound && !tags.tags.is_empty() && !missing_tag {
								assert_eq!(runs.len(), 1, "{}: planned by {runs:?}", case(request));
							}

							assert!(
								dispatched_from.len() <= 1,
								"{}: dispatched from {dispatched_from:?}",
								case(request)
							);
							assert!(
								dispatched_from.iter().all(|tag| !bound_tags.contains(tag)),
								"{}: dispatched from a pub.dev package's tag run {dispatched_from:?}",
								case(request)
							);

							checked += 1;
						}

						for run_ref in tags
							.tags
							.iter()
							.map(|tag| format!("refs/tags/{tag}"))
							.chain(other_refs.map(str::to_string))
						{
							let env_map = github_actions_env(&run_ref);
							let scopes = plan_publish_run_scopes(&requests, &tags, &env_map);

							for (request, scope) in requests.iter().zip(&scopes) {
								let bound = publish_is_tag_bound(request, &env_map);
								let case = format!("{} on {run_ref}: {scope:?}", case(request));

								if primary.is_some() && run_ref == "refs/tags/v2.0.0" && !bound {
									assert_eq!(*scope, PublishRunScope::Publish, "{case}");
								}

								if !any_bound {
									assert_eq!(*scope, PublishRunScope::Publish, "{case}");
								}

								if let PublishRunScope::OutOfScope { message } = scope {
									let home = message
										.split_once("publishes in the run on `refs/tags/")
										.and_then(|(_, rest)| rest.split('`').next())
										.unwrap_or_else(|| panic!("no publishing run: {case}"));
									let on_home = plan_publish_run_scopes(
										&requests,
										&tags,
										&github_actions_env(&format!("refs/tags/{home}")),
									);
									let index = requests
										.iter()
										.position(|candidate| {
											candidate.package_id == request.package_id
										})
										.unwrap_or_default();
									assert_eq!(
										on_home[index],
										PublishRunScope::Publish,
										"{case}: the run on `{home}` does not publish it"
									);
								}
							}
						}
					}
				}
			}
		}
	}

	assert_eq!(checked, 8 * 8 * 8 * 2 * 2 * 3);
}

/// Which packages a run publishes depends on the whole release, so a
/// `--package` or `--ecosystem` selection cannot pull a package into the
/// run on a pub.dev package's tag.
#[test]
fn assign_publish_run_scopes_plans_the_whole_release_before_the_selection() {
	let release = [
		ref_free_request("cli", Some("v2.0.1")),
		tag_bound_request("app_dart", Some("sdk/v1.1.0")),
		ref_free_request("web", Some("sdk/v1.1.0")),
	];
	let tags = release_tags(Some("v2.0.1"), &["sdk/v1.1.0"]);
	let env_map = github_actions_env("refs/tags/sdk/v1.1.0");
	let mut selected = vec![
		release[2].clone(),
		release[2].clone(),
		ref_free_request("outside", None),
	];

	assign_publish_run_scopes(&mut selected, &release, &tags, &env_map);

	assert_eq!(
		selected
			.iter()
			.map(|request| scope_summary(&request.run_scope))
			.collect::<Vec<_>>(),
		["out_of_scope", "out_of_scope", "publish"],
		"`web` is out of scope even when it is the only package selected (and across duplicate \
		 entries); a request outside the release keeps its scope"
	);
	assert_eq!(
		plan_summary(&selected[..1], &tags, &env_map),
		["publish"],
		"planning the selection alone would publish `web` here"
	);
}

#[test]
fn missing_release_tag_message_names_the_owner_section() {
	assert!(
		missing_release_tag_message("dart_pkg", Some("[group.sdk]"))
			.contains("Set `tag = true` on `[group.sdk]`")
	);
	assert!(
		missing_release_tag_message("dart_pkg", None)
			.contains("Set `tag = true` on `[package.dart_pkg]` (or on its group)")
	);
}

#[test]
fn summary_counts_deferred_and_out_of_scope_packages_and_resume_retries_them() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			not_attempted_outcome(
				&tag_bound_request("dart_pkg", None),
				PackagePublishStatus::DeferredToTag,
				"deferred".to_string(),
				disabled_trust_outcome(),
				None,
			),
			not_attempted_outcome(
				&publish_request("web"),
				PackagePublishStatus::SkippedOutOfScope,
				"out of scope".to_string(),
				disabled_trust_outcome(),
				None,
			),
		],
	};
	let summary = report.summary();

	assert_eq!(summary.deferred, 1);
	assert_eq!(summary.out_of_scope, 1);
	assert_eq!(summary.total(), 2);
	assert_eq!(summary.this_run(), 0);
	assert!(ensure_publish_report_succeeded(&report).is_ok());
	assert!(!package_publish_status_is_resumable_complete(
		PackagePublishStatus::SkippedOutOfScope
	));
	assert!(!package_publish_status_is_resumable_complete(
		PackagePublishStatus::DeferredToTag
	));
}

#[test]
fn summary_omits_zero_deferred_and_out_of_scope_counts_from_json() {
	let summary = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: true,
		packages: Vec::new(),
	}
	.summary();
	let json = serde_json::to_value(summary).unwrap_or_else(|error| panic!("summary: {error}"));

	assert_eq!(json.get("deferred"), None);
	assert_eq!(json.get("out_of_scope"), None);
}

fn registry_endpoints(server: &httpmock::MockServer) -> RegistryEndpoints {
	let mut endpoints = RegistryEndpoints::from_env();
	endpoints.npm_registry = server.base_url();
	endpoints.pub_dev_api = server.base_url();
	endpoints
}

fn successful_command_output(stdout: &str) -> CommandOutput {
	CommandOutput {
		success: true,
		stdout: stdout.to_string(),
		stderr: String::new(),
	}
}

/// Plan `requests` for the run in `env_map`, as a release whose primary tag
/// is `v2.0.0`, then execute them.
async fn execute_with_env(
	requests: &[PublishRequest],
	dry_run: bool,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut SequencedCommandExecutor,
) -> PackagePublishExecutionResult {
	let mut requests = requests.to_vec();
	let request_tags = requests
		.iter()
		.filter_map(|request| request.release_tag.as_ref())
		.map(|tag| tag.name.as_str())
		.collect::<Vec<_>>();
	let tags = release_tags(Some("v2.0.0"), &request_tags);
	let release = requests.clone();
	assign_publish_run_scopes(&mut requests, &release, &tags, env_map);

	try_execute_publish_requests_with_progress(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		dry_run,
		&requests,
		&registry_client().unwrap(),
		endpoints,
		env_map,
		executor,
		&build_publish_command_builder(),
		&PlaceholderManifestWriterRegistry::new(),
		&PublishReadinessRegistry::new(),
		&TestPublishTrustHandler,
		&NoopPublishProgressReporter,
	)
	.await
}

#[tokio::test(flavor = "multi_thread")]
async fn deferred_pub_dev_package_does_not_stop_later_packages() {
	let server = httpmock::MockServer::start();
	let dart_lookup = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/packages/dart_pkg");
		then.status(404);
	});
	let npm_lookup = server.mock(|when, then| {
		when.method(httpmock::Method::GET).path("/web");
		then.status(404);
	});
	let requests = [
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
		ref_free_request("web", Some("v2.0.0")),
	];
	let mut executor =
		SequencedCommandExecutor::new([Ok(successful_command_output("published web"))]);

	let report = execute_with_env(
		&requests,
		false,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/v2.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("a deferred package must not fail the run: {failure}"));

	dart_lookup.assert();
	npm_lookup.assert();
	assert_eq!(
		report.packages[0].status,
		PackagePublishStatus::DeferredToTag
	);
	assert_eq!(
		report.packages[0].deferred_to,
		Some(PublishDeferral {
			reason: PublishDeferralReason::WrongRef,
			tag: "dart_pkg/v1.0.0".to_string(),
			git_ref: "refs/tags/dart_pkg/v1.0.0".to_string(),
			command: Some(
				"gh workflow run publish.yml --repo owner/repo --ref dart_pkg/v1.0.0".to_string(),
			),
		})
	);
	assert!(
		report.packages[0]
			.message
			.starts_with("pub.dev only accepts dart_pkg 1.0.0 from a run on `refs/tags/dart_pkg/v1.0.0`, and this run is on `refs/tags/v2.0.0`"),
		"{}",
		report.packages[0].message
	);
	assert_eq!(report.packages[1].status, PackagePublishStatus::Published);
	assert_eq!(executor.commands.len(), 1, "only the npm package publishes");
	assert_eq!(executor.commands[0].program, "npm");
	assert!(ensure_publish_report_succeeded(&report).is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn pub_dev_tag_run_publishes_its_package_without_touching_other_registries() {
	let server = httpmock::MockServer::start();
	let dart_lookup = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/packages/dart_pkg");
		then.status(404);
	});
	let npm_lookup = server.mock(|when, then| {
		when.method(httpmock::Method::GET).path("/web");
		then.status(404);
	});
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
	];
	let mut executor =
		SequencedCommandExecutor::new([Ok(successful_command_output("published dart_pkg"))]);

	let report = execute_with_env(
		&requests,
		false,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/dart_pkg/v1.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("pub.dev tag run: {failure}"));

	npm_lookup.assert_calls(0);
	dart_lookup.assert();
	assert_eq!(
		report.packages[0].status,
		PackagePublishStatus::SkippedOutOfScope
	);
	assert!(
		report.packages[0]
			.message
			.contains("web 1.0.0 publishes in the run on `refs/tags/v2.0.0`"),
		"{}",
		report.packages[0].message
	);
	assert_eq!(report.packages[1].status, PackagePublishStatus::Published);
	assert_eq!(
		executor.commands.len(),
		1,
		"only the dart package publishes"
	);
	assert_eq!(executor.commands[0].program, "dart");
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_release_tag_aborts_the_release_before_any_registry_request() {
	let server = httpmock::MockServer::start();
	let lookups = server.mock(|when, then| {
		when.method(httpmock::Method::GET);
		then.status(404);
	});
	let requests = [
		ref_free_request("web", Some("v2.0.0")),
		tag_bound_request("untagged_dart", None),
	];
	let mut executor = SequencedCommandExecutor::new([]);

	let failure = execute_with_env(
		&requests,
		false,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/v2.0.0"),
		&mut executor,
	)
	.await
	.expect_err("an unpublishable package must abort before anything publishes");

	lookups.assert_calls(0);
	assert!(executor.commands.is_empty());
	assert!(failure.report().packages.is_empty());
	assert!(
		failure
			.render()
			.contains("`untagged_dart` publishes to pub.dev with trusted publishing")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_marks_missing_release_tag_blocked_and_keeps_planning() {
	let server = httpmock::MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET);
		then.status(404);
	});
	let requests = [
		tag_bound_request("untagged_dart", None),
		ref_free_request("web", Some("v2.0.0")),
	];
	let mut executor =
		SequencedCommandExecutor::new([Ok(successful_command_output("npm dry run"))]);

	let report = execute_with_env(
		&requests,
		true,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/v2.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("dry run: {failure}"));

	assert_eq!(report.packages[0].status, PackagePublishStatus::Blocked);
	assert!(report.packages[0].message.contains("Set `tag = true`"));
	assert_eq!(report.packages[1].status, PackagePublishStatus::Planned);
}

#[tokio::test(flavor = "multi_thread")]
async fn deferred_package_with_an_existing_version_skips_duplicate_rejection() {
	let server = httpmock::MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/packages/dart_pkg");
		then.status(200)
			.json_body(serde_json::json!({ "versions": [{ "version": "1.0.0" }] }));
	});
	let mut request = tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0"));
	request.fail_on_duplicate = true;
	let mut executor = SequencedCommandExecutor::new([]);

	let report = execute_with_env(
		&[request],
		false,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/v2.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("existing deferred version: {failure}"));

	assert_eq!(
		report.packages[0].status,
		PackagePublishStatus::SkippedExisting
	);
}

/// A failure in a run on a pub.dev package's tag leaves the packages other
/// runs publish as they were planned, instead of blocking them.
#[tokio::test(flavor = "multi_thread")]
async fn failure_keeps_packages_other_runs_publish_out_of_scope_or_deferred() {
	let server = httpmock::MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET);
		then.status(404);
	});
	let failed_upload = || {
		Ok(CommandOutput {
			success: false,
			stdout: String::new(),
			stderr: "upload rejected".to_string(),
		})
	};

	// On the pub.dev package's tag, the packages after the failure keep their
	// own statuses: external stays skipped_external, and other runs' packages
	// stay out of scope.
	let mut external = ref_free_request("ext_pkg", Some("v2.0.0"));
	external.mode = PublishMode::External;
	let mut executor = SequencedCommandExecutor::new([failed_upload()]);
	let report = execute_with_env(
		&[
			tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
			external,
			ref_free_request("web", Some("v2.0.0")),
			tag_bound_request("other_dart", Some("other_dart/v3.0.0")),
		],
		false,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/dart_pkg/v1.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("a failed upload is reported in the run: {failure}"));
	let statuses = |report: &PackagePublishReport| {
		report
			.packages
			.iter()
			.map(|outcome| (outcome.package.clone(), outcome.status))
			.collect::<Vec<_>>()
	};
	assert_eq!(
		statuses(&report),
		[
			("dart_pkg".to_string(), PackagePublishStatus::Failed),
			("ext_pkg".to_string(), PackagePublishStatus::SkippedExternal),
			("web".to_string(), PackagePublishStatus::SkippedOutOfScope),
			(
				"other_dart".to_string(),
				PackagePublishStatus::SkippedOutOfScope
			),
		]
	);
	assert_eq!(
		report.packages[1].message,
		"package opted out of built-in publishing"
	);
	assert!(
		report.packages[2]
			.message
			.contains("publishes in the run on `refs/tags/v2.0.0`"),
		"{}",
		report.packages[2].message
	);

	// On the primary tag, a later pub.dev package stays deferred to its tag,
	// and a later package this run publishes is blocked.
	let mut executor = SequencedCommandExecutor::new([failed_upload()]);
	let report = execute_with_env(
		&[
			ref_free_request("web", Some("v2.0.0")),
			tag_bound_request("dart_pkg", Some("dart_pkg/v1.0.0")),
			ref_free_request("cli", Some("v2.0.0")),
		],
		false,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/v2.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("a failed upload is reported in the run: {failure}"));
	assert_eq!(
		statuses(&report),
		[
			("web".to_string(), PackagePublishStatus::Failed),
			("dart_pkg".to_string(), PackagePublishStatus::DeferredToTag),
			("cli".to_string(), PackagePublishStatus::Blocked),
		]
	);
	assert_eq!(
		report.packages[1]
			.deferred_to
			.as_ref()
			.map(|deferral| deferral.tag.as_str()),
		Some("dart_pkg/v1.0.0")
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

#[tokio::test(flavor = "multi_thread")]
async fn pub_token_publish_keeps_the_token_and_mints_no_oidc_token() {
	let server = httpmock::MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET).path("/packages/pkg");
		then.status(404);
	});
	let oidc = server.mock(|when, then| {
		when.path("/token");
		then.status(200)
			.json_body(serde_json::json!({ "count": 1, "value": "actions-jwt" }));
	});
	let mut env_map = actions_oidc_env_map(&server.base_url(), "runner-secret");
	env_map.insert(PUB_TOKEN_ENV_VAR.to_string(), "pub-token".to_string());
	let mut executor =
		SequencedCommandExecutor::new([Ok(successful_command_output("published pkg"))]);

	let report = execute_with_env(
		&[trusted_pub_dev_publish_request("pkg")],
		false,
		&registry_endpoints(&server),
		&env_map,
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("token publish: {failure}"));

	oidc.assert_calls(0);
	assert_eq!(executor.commands.len(), 1, "only `dart pub publish` runs");
	assert_eq!(executor.commands[0].program, "dart");
	assert_eq!(executor.commands[0].env.get(PUB_TOKEN_ENV_VAR), None);
	assert_eq!(report.packages[0].status, PackagePublishStatus::Published);
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_blocks_missing_release_tag_before_checking_the_registry() {
	let server = httpmock::MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/packages/untagged_dart");
		then.status(200)
			.json_body(serde_json::json!({ "versions": [{ "version": "1.0.0" }] }));
	});
	let mut executor = SequencedCommandExecutor::new([]);

	let report = execute_with_env(
		&[tag_bound_request("untagged_dart", None)],
		true,
		&registry_endpoints(&server),
		&github_actions_env("refs/tags/v2.0.0"),
		&mut executor,
	)
	.await
	.unwrap_or_else(|failure| panic!("dry run: {failure}"));

	// A real run aborts on the missing tag before any registry request; the
	// dry run reports the same problem instead of `already exists`.
	assert_eq!(report.packages[0].status, PackagePublishStatus::Blocked);
	lookup.assert_calls(0);
}

fn rate_limit_test_outcome(
	package: &str,
	status: PackagePublishStatus,
	rate_limit: Option<PublishRateLimitHit>,
) -> PackagePublishOutcome {
	PackagePublishOutcome {
		package: package.to_string(),
		ecosystem: Ecosystem::Dart,
		registry: RegistryKind::PubDev.to_string(),
		version: "1.0.0".to_string(),
		status,
		message: "outcome".to_string(),
		placeholder: false,
		trusted_publishing: disabled_trust_outcome(),
		command: None,
		stdout: None,
		stderr: None,
		rate_limit,
		deferred_to: None,
	}
}

#[test]
fn rate_limit_message_counts_only_the_packages_this_run_publishes() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			rate_limit_test_outcome("done", PackagePublishStatus::Published, None),
			rate_limit_test_outcome("web", PackagePublishStatus::SkippedOutOfScope, None),
			rate_limit_test_outcome("cli", PackagePublishStatus::SkippedOutOfScope, None),
			rate_limit_test_outcome("other_dart", PackagePublishStatus::DeferredToTag, None),
			rate_limit_test_outcome(
				"limited",
				PackagePublishStatus::Failed,
				Some(PublishRateLimitHit {
					reason: None,
					retry_at: None,
					docs_url: None,
				}),
			),
		],
	};
	let now = chrono::DateTime::parse_from_rfc3339("2026-10-20T10:53:00Z")
		.unwrap_or_else(|error| panic!("now: {error}"))
		.with_timezone(&chrono::Utc);

	let error = ensure_publish_report_succeeded_at(&report, now)
		.expect_err("rate-limited report should fail")
		.to_string();

	assert!(error.contains("1 of 2 packages published"), "{error}");
	assert!(error.contains("(1 package left)"), "{error}");
}

#[test]
fn failed_run_summary_counts_deferred_and_out_of_scope_packages() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			rate_limit_test_outcome("web", PackagePublishStatus::SkippedOutOfScope, None),
			rate_limit_test_outcome("other_dart", PackagePublishStatus::DeferredToTag, None),
			rate_limit_test_outcome("broken", PackagePublishStatus::Failed, None),
		],
	};

	let error = ensure_publish_report_succeeded(&report)
		.expect_err("failed report")
		.to_string();

	assert!(error.contains("3 total"), "{error}");
	assert!(error.contains("1 deferred to release tags"), "{error}");
	assert!(error.contains("1 out of scope for this ref"), "{error}");
}
