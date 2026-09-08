use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::RegistryKind;
use monochange_core::TrustedPublishingSettings;
use monochange_publish::PublishRequest;
use tempfile::TempDir;

use super::*;
use crate::trusted_publishing_readiness::TrustedPublishingReadinessStatus;
use crate::trusted_publishing_readiness::check_trusted_publishing_readiness;
use crate::trusted_publishing_readiness::trusted_publishing_project_blocker_message;

fn sample_request(registry: RegistryKind, root: &TempDir) -> PublishRequest {
	PublishRequest {
		package_id: "pkg".to_string(),
		package_name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: root.path().join("package.json"),
		package_root: root.path().to_path_buf(),
		registry,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: monochange_core::PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: "placeholder".to_string(),
	}
}

fn github_ci_env() -> BTreeMap<String, String> {
	BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_REPOSITORY".to_string(), "acme/widgets".to_string()),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"acme/widgets/.github/workflows/release.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "publish".to_string()),
	])
}

fn spawn_registry_mock(response: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind registry mock: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("registry mock address: {error}"));
	let thread = std::thread::spawn(move || {
		let (mut stream, _) = listener
			.accept()
			.unwrap_or_else(|error| panic!("accept registry request: {error}"));
		let mut request = [0_u8; 2048];
		std::io::Read::read(&mut stream, &mut request)
			.unwrap_or_else(|error| panic!("read registry request: {error}"));
		std::io::Write::write_all(&mut stream, response)
			.unwrap_or_else(|error| panic!("write registry response: {error}"));
	});
	(format!("http://{address}"), thread)
}

const NPM_PACKAGE_BODY: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 25\r\nConnection: close\r\n\r\n{\"versions\":{\"1.0.0\":{}}}";
const REGISTRY_NOT_FOUND: &[u8] =
	b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const REGISTRY_SERVER_ERROR: &[u8] =
	b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

#[tokio::test]
async fn disabled_trust_publishing_reports_disabled_status() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.enabled = false;

	let readiness =
		check_trusted_publishing_readiness(root.path(), None, &request, &BTreeMap::new(), None)
			.await;

	assert_eq!(readiness.status, TrustedPublishingReadinessStatus::Disabled);
}

#[tokio::test]
async fn local_environment_degrades_to_manual_verification() {
	let root = tempfile::tempdir().unwrap();
	let request = sample_request(RegistryKind::Npm, &root);

	let readiness =
		check_trusted_publishing_readiness(root.path(), None, &request, &BTreeMap::new(), None)
			.await;

	assert_eq!(
		readiness.status,
		TrustedPublishingReadinessStatus::ManualVerificationRequired
	);
	assert!(readiness.message.contains("CI workflow"));
}

#[tokio::test]
async fn unsupported_provider_blocks_trusted_publishing() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	let env_map = BTreeMap::from([("CIRCLECI".to_string(), "true".to_string())]);

	let readiness =
		check_trusted_publishing_readiness(root.path(), None, &request, &env_map, None).await;

	assert_eq!(readiness.status, TrustedPublishingReadinessStatus::Blocked);
	assert!(readiness.message.contains("not supported"));
	assert!(
		readiness
			.message
			.contains("https://www.npmjs.com/package/pkg/access")
	);
}

#[tokio::test]
async fn missing_workflow_file_blocks_github_ci_context() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());

	let readiness =
		check_trusted_publishing_readiness(root.path(), None, &request, &github_ci_env(), None)
			.await;

	assert_eq!(readiness.status, TrustedPublishingReadinessStatus::Blocked);
	assert!(readiness.message.contains("release.yml"));
}

#[tokio::test]
async fn existing_workflow_file_passes_project_side_checks() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
	std::fs::write(
		root.path().join(".github/workflows/release.yml"),
		"jobs: {}",
	)
	.unwrap();

	// The project-side checks pass, so the remaining finding comes from the
	// registry side (offline environment degrades to manual verification).
	let readiness =
		check_trusted_publishing_readiness(root.path(), None, &request, &github_ci_env(), None)
			.await;

	assert_eq!(
		readiness.status,
		TrustedPublishingReadinessStatus::ManualVerificationRequired
	);
	assert!(readiness.message.contains("registry"));
}

#[tokio::test]
async fn project_blocker_message_returns_blocked_messages_only() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.enabled = false;
	assert!(
		trusted_publishing_project_blocker_message(root.path(), None, &request, &BTreeMap::new())
			.is_none()
	);

	request.trusted_publishing = TrustedPublishingSettings {
		repository: Some("acme/widgets".to_string()),
		workflow: Some("missing.yml".to_string()),
		..TrustedPublishingSettings::default()
	};
	std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();

	let blocker =
		trusted_publishing_project_blocker_message(root.path(), None, &request, &github_ci_env());
	assert!(blocker.is_some_and(|message| message.contains("missing.yml")));
}

#[tokio::test]
async fn incomplete_ci_identity_blocks_in_ci() {
	let root = tempfile::tempdir().unwrap();
	let request = sample_request(RegistryKind::Npm, &root);
	// CI identity is detectable but the publish-time environment cannot
	// verify it, which fails the publish outright in CI.
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_REPOSITORY".to_string(), "acme/widgets".to_string()),
	]);

	let readiness =
		check_trusted_publishing_readiness(root.path(), None, &request, &env_map, None).await;

	assert_eq!(readiness.status, TrustedPublishingReadinessStatus::Blocked);
	assert!(readiness.message.contains("incomplete"));
}

#[tokio::test]
async fn source_configuration_supplies_repository_context() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
	std::fs::write(
		root.path().join(".github/workflows/release.yml"),
		"jobs: {}",
	)
	.unwrap();
	let source = SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		owner: "acme".to_string(),
		repo: "widgets".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	let env_map = github_ci_env();

	// The source configuration supplies the repository context, the workflow
	// file exists, and the registry probe confirms the package exists; the
	// finding stays non-blocking and points at the npm setup URL.
	let (base_url, server) = spawn_registry_mock(NPM_PACKAGE_BODY);
	let client = monochange_publish::registry_client().unwrap();
	let endpoints = RegistryEndpoints {
		npm_registry: base_url,
		..RegistryEndpoints::from_env()
	};

	let readiness = check_trusted_publishing_readiness(
		root.path(),
		Some(&source),
		&request,
		&env_map,
		Some((&client, &endpoints)),
	)
	.await;

	assert_eq!(
		readiness.status,
		TrustedPublishingReadinessStatus::ManualVerificationRequired
	);
	assert!(readiness.message.contains("pkg"));
	server
		.join()
		.unwrap_or_else(|_| panic!("registry mock thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_lookup_failure_degrades_to_manual_verification() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
	std::fs::write(
		root.path().join(".github/workflows/release.yml"),
		"jobs: {}",
	)
	.unwrap();

	let (base_url, server) = spawn_registry_mock(REGISTRY_SERVER_ERROR);
	let client = monochange_publish::registry_client().unwrap();
	let endpoints = RegistryEndpoints {
		npm_registry: base_url,
		..RegistryEndpoints::from_env()
	};

	let readiness = check_trusted_publishing_readiness(
		root.path(),
		None,
		&request,
		&github_ci_env(),
		Some((&client, &endpoints)),
	)
	.await;

	assert_eq!(
		readiness.status,
		TrustedPublishingReadinessStatus::ManualVerificationRequired
	);
	assert!(readiness.message.contains("lookup failed"));
	server
		.join()
		.unwrap_or_else(|_| panic!("registry mock thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_gitlab_npm_identity_blocks_on_unresolvable_workflow() {
	let root = tempfile::tempdir().unwrap();
	let request = sample_request(RegistryKind::Npm, &root);
	// GitLab CI is a supported npm trusted-publishing provider, but without
	// a configured repository/workflow the GitHub trust context cannot
	// resolve and the publish would fail.
	let env_map = BTreeMap::from([
		("GITLAB_CI".to_string(), "true".to_string()),
		("CI_PROJECT_PATH".to_string(), "acme/widgets".to_string()),
		("CI_JOB_ID".to_string(), "42".to_string()),
	]);

	// The trust context fails before the registry probe, so the mock server
	// never receives a request; drop its join handle instead of joining.
	let (base_url, _server) = spawn_registry_mock(NPM_PACKAGE_BODY);
	let client = monochange_publish::registry_client().unwrap();
	let endpoints = RegistryEndpoints {
		npm_registry: base_url,
		..RegistryEndpoints::from_env()
	};

	let readiness = check_trusted_publishing_readiness(
		root.path(),
		None,
		&request,
		&env_map,
		Some((&client, &endpoints)),
	)
	.await;

	assert_eq!(readiness.status, TrustedPublishingReadinessStatus::Blocked);
	assert!(readiness.message.contains("repository"));
}

#[tokio::test(flavor = "multi_thread")]
async fn never_published_package_blocks_trusted_publishing() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
	std::fs::write(
		root.path().join(".github/workflows/release.yml"),
		"jobs: {}",
	)
	.unwrap();

	let (base_url, server) = spawn_registry_mock(REGISTRY_NOT_FOUND);
	let client = monochange_publish::registry_client().unwrap();
	let endpoints = RegistryEndpoints {
		npm_registry: base_url,
		..RegistryEndpoints::from_env()
	};

	let readiness =
		crate::trusted_publishing_readiness::registry_side_readiness(&request, &client, &endpoints)
			.await;

	assert_eq!(readiness.status, TrustedPublishingReadinessStatus::Blocked);
	assert!(readiness.message.contains("placeholder-publish"));
	server
		.join()
		.unwrap_or_else(|_| panic!("registry mock thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_package_requires_manual_registry_side_verification() {
	let root = tempfile::tempdir().unwrap();
	let mut request = sample_request(RegistryKind::Npm, &root);
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
	std::fs::write(
		root.path().join(".github/workflows/release.yml"),
		"jobs: {}",
	)
	.unwrap();

	let (base_url, server) = spawn_registry_mock(NPM_PACKAGE_BODY);
	let client = monochange_publish::registry_client().unwrap();
	let endpoints = RegistryEndpoints {
		npm_registry: base_url,
		..RegistryEndpoints::from_env()
	};

	let readiness =
		crate::trusted_publishing_readiness::registry_side_readiness(&request, &client, &endpoints)
			.await;

	assert_eq!(
		readiness.status,
		TrustedPublishingReadinessStatus::ManualVerificationRequired
	);
	assert!(readiness.message.contains("npmjs.com/package/pkg/access"));
	server
		.join()
		.unwrap_or_else(|_| panic!("registry mock thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn unprobeable_registries_report_manual_verification() {
	let root = tempfile::tempdir().unwrap();
	let request = sample_request(RegistryKind::Jsr, &root);
	let client = monochange_publish::registry_client().unwrap();
	let endpoints = RegistryEndpoints::from_env();

	let readiness =
		crate::trusted_publishing_readiness::registry_side_readiness(&request, &client, &endpoints)
			.await;

	assert_eq!(
		readiness.status,
		TrustedPublishingReadinessStatus::ManualVerificationRequired
	);
	assert!(readiness.message.contains("not probe-able"));
}
