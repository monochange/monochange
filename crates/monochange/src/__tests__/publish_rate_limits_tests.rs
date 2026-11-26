#![allow(clippy::disallowed_methods)]
use std::fs;

use httpmock::Method::GET;
use httpmock::MockServer;
use monochange_core::DependencyKind;
use monochange_core::PackagePublicationTarget;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishRateLimitBatch;
use monochange_core::PublishRegistry;
use monochange_core::PublishTimeoutSettings;
use monochange_core::RateLimitConfidence;
use monochange_core::RegistryRateLimitPolicy;
use monochange_core::RegistryRateLimitWindowPlan;
use monochange_core::TrustedPublishingSettings;
use monochange_core::VersionSource;
use monochange_publish::RegistryEndpoints;
use monochange_publish::filter_pending_publish_requests_with_transport;
use monochange_test_helpers::install_rustls_ring_provider;
use semver::Version;
use tempfile::tempdir;

use super::*;
use crate::discover_workspace;

#[tokio::test(flavor = "multi_thread")]
async fn enforce_publish_rate_limits_blocks_multi_batch_runs_when_enabled() {
	let requests = (0..13)
		.map(|index| {
			package_publish::PublishRequest {
				package_id: format!("pkg-{index}"),
				package_name: format!("pkg-{index}"),
				ecosystem: monochange_core::Ecosystem::Dart,
				manifest_path: Path::new("pkg-a/pubspec.yaml").to_path_buf(),
				package_root: Path::new("pkg-a").to_path_buf(),
				registry: RegistryKind::PubDev,
				package_manager: None,
				package_metadata: BTreeMap::new(),
				mode: PublishMode::Builtin,
				flow: monochange_core::PublishFlow::Direct,
				version: Version::new(1, 0, 0).to_string(),
				placeholder: false,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
				timeout: PublishTimeoutSettings::default(),
				placeholder_readme: String::new(),
				fail_on_duplicate: false,
				release_tag: None,
				run_scope: monochange_publish::PublishRunScope::Publish,
			}
		})
		.collect::<Vec<_>>();
	let mut report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);
	let mut second_window = report.windows[0].clone();
	second_window.registry = RegistryKind::CratesIo;
	report.windows.push(second_window);

	let configuration = WorkspaceConfiguration {
		root_path: Path::new(".").to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: (0..13)
			.map(|index| {
				monochange_core::PackageDefinition {
					id: format!("pkg-{index}"),
					path: Path::new("pkg-a").to_path_buf(),
					package_type: monochange_core::PackageType::Dart,
					changelog: None,
					excluded_changelog_types: Vec::new(),
					bump_propagation: None,
					empty_update_message: None,
					release_title: None,
					changelog_version_title: None,
					versioned_files: Vec::new(),
					ignore_ecosystem_versioned_files: false,
					ignored_paths: Vec::new(),
					additional_paths: Vec::new(),
					tag: false,
					release: false,
					version_format: monochange_core::VersionFormat::default(),
					version_source: VersionSource::default(),
					initial_version: None,
					bump_ceiling: None,
					classification_enforced: None,

					floating_tags: Vec::new(),
					publish: monochange_core::PublishSettings {
						rate_limits: monochange_core::PublishRateLimitSettings { enforce: true },
						..monochange_core::PublishSettings::default()
					},
					cli: None,
					display_version: None,
					values: BTreeMap::new(),
				}
			})
			.collect(),
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
	};
	let error = enforce_publish_rate_limits(&configuration, &report, PublishRateLimitMode::Publish)
		.unwrap_err();
	let error = error.to_string();
	assert!(error.contains("blocked this run"));
	assert!(error.contains("; "));
}

#[test]
fn enforce_publish_rate_limits_ignores_deferred_and_out_of_scope_packages() {
	let pub_dev_requests = (0..13)
		.map(|index| {
			let id = format!("pkg-{index}");
			package_publish::PublishRequest {
				package_id: id.clone(),
				package_name: id.clone(),
				ecosystem: monochange_core::Ecosystem::Dart,
				manifest_path: Path::new("pkg/pubspec.yaml").to_path_buf(),
				package_root: Path::new("pkg").to_path_buf(),
				registry: RegistryKind::PubDev,
				package_manager: None,
				package_metadata: BTreeMap::new(),
				mode: PublishMode::Builtin,
				flow: monochange_core::PublishFlow::Direct,
				version: Version::new(1, 0, 0).to_string(),
				placeholder: false,
				trusted_publishing: TrustedPublishingSettings {
					enabled: true,
					..TrustedPublishingSettings::default()
				},
				attestations: PublishAttestationSettings::default(),
				timeout: PublishTimeoutSettings::default(),
				placeholder_readme: String::new(),
				fail_on_duplicate: false,
				release_tag: Some(monochange_publish::PublishReleaseTag {
					name: format!("{id}/v1.0.0"),
				}),
				run_scope: PublishRunScope::Publish,
			}
		})
		.collect::<Vec<_>>();
	let npm_request = package_publish::PublishRequest {
		package_id: "web".to_string(),
		package_name: "web".to_string(),
		ecosystem: monochange_core::Ecosystem::Npm,
		manifest_path: Path::new("web/package.json").to_path_buf(),
		package_root: Path::new("web").to_path_buf(),
		registry: RegistryKind::Npm,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		flow: monochange_core::PublishFlow::Direct,
		version: Version::new(1, 0, 0).to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		placeholder_readme: String::new(),
		fail_on_duplicate: false,
		release_tag: Some(monochange_publish::PublishReleaseTag {
			name: "v1.0.0".to_string(),
		}),
		run_scope: PublishRunScope::Publish,
	};
	let mut release_requests = pub_dev_requests;
	release_requests.push(npm_request);
	let tags = monochange_publish::PublishReleaseTags {
		primary: Some("v1.0.0".to_string()),
		tags: (0..13)
			.map(|index| format!("pkg-{index}/v1.0.0"))
			.chain(std::iter::once("v1.0.0".to_string()))
			.collect(),
	};
	let configuration = WorkspaceConfiguration {
		root_path: Path::new(".").to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: (0..13)
			.map(|index| {
				monochange_core::PackageDefinition {
					id: format!("pkg-{index}"),
					path: Path::new("pkg").to_path_buf(),
					package_type: monochange_core::PackageType::Dart,
					changelog: None,
					excluded_changelog_types: Vec::new(),
					bump_propagation: None,
					empty_update_message: None,
					release_title: None,
					changelog_version_title: None,
					versioned_files: Vec::new(),
					ignore_ecosystem_versioned_files: false,
					ignored_paths: Vec::new(),
					additional_paths: Vec::new(),
					tag: true,
					release: false,
					version_format: monochange_core::VersionFormat::default(),
					version_source: VersionSource::default(),
					initial_version: None,
					bump_ceiling: None,
					classification_enforced: None,
					floating_tags: Vec::new(),
					publish: monochange_core::PublishSettings {
						rate_limits: monochange_core::PublishRateLimitSettings { enforce: true },
						..monochange_core::PublishSettings::default()
					},
					cli: None,
					display_version: None,
					values: BTreeMap::new(),
				}
			})
			.collect(),
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
	};

	// On the primary tag all 13 pub.dev packages are deferred, so enforcement
	// does not block the run.
	let primary_env = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_REF".to_string(), "refs/tags/v1.0.0".to_string()),
		("GITHUB_REF_TYPE".to_string(), "tag".to_string()),
		("GITHUB_EVENT_NAME".to_string(), "push".to_string()),
	]);
	let mut on_primary = release_requests.clone();
	monochange_publish::assign_publish_run_scopes(
		&mut on_primary,
		&release_requests,
		&tags,
		&primary_env,
	);
	let primary_report =
		plan_publish_rate_limits_for_requests(&on_primary, RateLimitOperation::Publish, false);
	assert_eq!(
		primary_report
			.windows
			.iter()
			.map(|window| window.registry)
			.collect::<Vec<_>>(),
		vec![RegistryKind::Npm]
	);
	assert!(
		enforce_publish_rate_limits(
			&configuration,
			&primary_report,
			PublishRateLimitMode::Publish
		)
		.is_ok()
	);

	// On one pub.dev package's own tag, the other 12 pub.dev packages are out
	// of scope, so only 1 pub.dev upload is planned and enforcement passes.
	let package_tag_env = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_REF".to_string(),
			"refs/tags/pkg-0/v1.0.0".to_string(),
		),
		("GITHUB_REF_TYPE".to_string(), "tag".to_string()),
		("GITHUB_EVENT_NAME".to_string(), "push".to_string()),
	]);
	let mut on_package_tag = release_requests.clone();
	monochange_publish::assign_publish_run_scopes(
		&mut on_package_tag,
		&release_requests,
		&tags,
		&package_tag_env,
	);
	let package_tag_report =
		plan_publish_rate_limits_for_requests(&on_package_tag, RateLimitOperation::Publish, false);
	assert_eq!(package_tag_report.windows.len(), 1);
	assert_eq!(package_tag_report.windows[0].registry, RegistryKind::PubDev);
	assert_eq!(package_tag_report.windows[0].pending, 1);
	assert!(package_tag_report.windows[0].fits_single_window);
	assert!(
		enforce_publish_rate_limits(
			&configuration,
			&package_tag_report,
			PublishRateLimitMode::Publish
		)
		.is_ok()
	);

	// Control: outside GitHub Actions nothing is deferred, so all 13 exceed
	// pub.dev's single window and enforcement blocks the run.
	let mut unscoped = release_requests.clone();
	monochange_publish::assign_publish_run_scopes(
		&mut unscoped,
		&release_requests,
		&tags,
		&BTreeMap::new(),
	);
	let unscoped_report =
		plan_publish_rate_limits_for_requests(&unscoped, RateLimitOperation::Publish, false);
	let error = enforce_publish_rate_limits(
		&configuration,
		&unscoped_report,
		PublishRateLimitMode::Publish,
	)
	.expect_err("13 in-scope pub.dev packages must trigger rate-limit enforcement");
	assert!(error.to_string().contains("blocked this run"), "{error}");
}
