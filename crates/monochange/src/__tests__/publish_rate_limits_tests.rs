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
