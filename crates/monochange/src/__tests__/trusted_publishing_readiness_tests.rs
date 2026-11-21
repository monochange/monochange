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
		flow: monochange_core::PublishFlow::Direct,
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
