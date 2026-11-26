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

#[tokio::test(flavor = "multi_thread")]
async fn enforce_publish_rate_limits_ignores_deferred_and_out_of_scope_packages() {
	install_rustls_ring_provider();
	let root = tempdir().expect("tempdir:");
	let mut config = String::from(
		r#"[package.cli]
path = "crates/cli"
type = "cargo"
tag = true
version_format = "primary"

[package.cli.publish.rate_limits]
enforce = true
"#,
	);
	let cli_dir = root.path().join("crates/cli");
	fs::create_dir_all(cli_dir.join("src")).expect("mkdir:");
	fs::write(
		cli_dir.join("Cargo.toml"),
		"[package]\nname = \"cli\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.expect("Cargo.toml:");
	fs::write(cli_dir.join("src/lib.rs"), "").expect("lib.rs:");

	for index in 0..13 {
		let id = format!("pkg_{index}");
		let _ = write!(
			config,
			"\n[package.{id}]\npath = \"packages/{id}\"\ntype = \"dart\"\ntag = true\n\n[package.{id}.publish.rate_limits]\nenforce = true\n"
		);
		let pkg_dir = root.path().join("packages").join(&id);
		fs::create_dir_all(&pkg_dir).expect("mkdir:");
		fs::write(
			pkg_dir.join("pubspec.yaml"),
			format!("name: {id}\nversion: 1.0.0\n"),
		)
		.expect("pubspec.yaml:");
	}
	fs::write(root.path().join("monochange.toml"), config).expect("monochange.toml:");

	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration:");
	let discovery =
		crate::discover_release_workspace(root.path(), &configuration).expect("discovery:");
	let server = MockServer::start();
	let crates_io_lookup = server.mock(|when, then| {
		when.method(GET).path_prefix("/crates-io/crates/");
		then.status(404)
			.json_body(serde_json::json!({ "errors": [] }));
	});
	let pub_dev_lookup = server.mock(|when, then| {
		when.method(GET).path_prefix("/pub/packages/");
		then.status(404)
			.json_body(serde_json::json!({ "error": { "code": "NotFound" } }));
	});
	let endpoints = RegistryEndpoints {
		crates_io_api: server.url("/crates-io"),
		crates_io_index: server.url("/crates-index"),
		pub_dev_api: server.url("/pub"),
		..RegistryEndpoints::from_env()
	};
	let client = reqwest::Client::new();
	let selected = BTreeSet::new();

	// On the primary tag all 13 pub.dev packages are deferred; only `cli`
	// (configured with `rate_limits.enforce = true`) is probed and planned, so
	// enforcement reaches the window check and passes.
	let primary_env = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_REF".to_string(), "refs/tags/v1.0.0".to_string()),
		("GITHUB_REF_TYPE".to_string(), "tag".to_string()),
		("GITHUB_EVENT_NAME".to_string(), "push".to_string()),
	]);
	let primary_requests = build_scoped_release_plan_requests(
		root.path(),
		&configuration,
		None,
		&discovery.packages,
		&selected,
		true,
		&primary_env,
	)
	.await
	.expect("primary scoped requests:");
	let primary_pending =
		filter_pending_publish_requests_with_transport(&primary_requests, &client, &endpoints)
			.await
			.expect("primary pending:");
	assert_eq!(crates_io_lookup.calls(), 1);
	assert_eq!(
		pub_dev_lookup.calls(),
		0,
		"deferred pub.dev packages must not be probed"
	);
	let primary_report = plan_publish_rate_limits_for_dependency_ordered_requests(
		&primary_pending,
		&discovery.packages,
		RateLimitOperation::Publish,
		false,
	);
	assert_eq!(primary_report.windows.len(), 1);
	assert_eq!(primary_report.windows[0].registry, RegistryKind::CratesIo);
	assert_eq!(primary_report.windows[0].pending, 1);
	assert!(primary_report.windows[0].fits_single_window);
	assert!(
		primary_report
			.batches
			.iter()
			.flat_map(|batch| batch.packages.iter())
			.any(|package| {
				configuration
					.package_by_id(package)
					.is_some_and(|definition| definition.publish.rate_limits.enforce)
			}),
		"cli must be an enforced package so enforce_publish_rate_limits checks the window"
	);
	assert!(
		enforce_publish_rate_limits(
			&configuration,
			&primary_report,
			PublishRateLimitMode::Publish
		)
		.is_ok()
	);

	// On one pub.dev package's own tag, the other 12 pub.dev packages and
	// `cli` are out of scope, so only `pkg_0` is probed and planned.
	let package_tag_env = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_REF".to_string(),
			"refs/tags/pkg_0/v1.0.0".to_string(),
		),
		("GITHUB_REF_TYPE".to_string(), "tag".to_string()),
		("GITHUB_EVENT_NAME".to_string(), "push".to_string()),
	]);
	let package_tag_requests = build_scoped_release_plan_requests(
		root.path(),
		&configuration,
		None,
		&discovery.packages,
		&selected,
		true,
		&package_tag_env,
	)
	.await
	.expect("package tag scoped requests:");
	let package_tag_pending =
		filter_pending_publish_requests_with_transport(&package_tag_requests, &client, &endpoints)
			.await
			.expect("package tag pending:");
	assert_eq!(
		crates_io_lookup.calls(),
		1,
		"out-of-scope cli must not be probed"
	);
	assert_eq!(
		pub_dev_lookup.calls(),
		1,
		"only pkg_0 must be probed on its tag"
	);
	let package_tag_report = plan_publish_rate_limits_for_dependency_ordered_requests(
		&package_tag_pending,
		&discovery.packages,
		RateLimitOperation::Publish,
		false,
	);
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

	// Control: outside GitHub Actions nothing is deferred, so all 13 pub.dev
	// packages exceed pub.dev's single 12-package window and enforcement blocks.
	let unscoped_requests = build_scoped_release_plan_requests(
		root.path(),
		&configuration,
		None,
		&discovery.packages,
		&selected,
		true,
		&BTreeMap::new(),
	)
	.await
	.expect("unscoped requests:");
	let unscoped_pending =
		filter_pending_publish_requests_with_transport(&unscoped_requests, &client, &endpoints)
			.await
			.expect("unscoped pending:");
	let unscoped_report = plan_publish_rate_limits_for_dependency_ordered_requests(
		&unscoped_pending,
		&discovery.packages,
		RateLimitOperation::Publish,
		false,
	);
	let error = enforce_publish_rate_limits(
		&configuration,
		&unscoped_report,
		PublishRateLimitMode::Publish,
	)
	.expect_err("13 in-scope pub.dev packages must trigger rate-limit enforcement");
	assert!(error.to_string().contains("blocked this run"), "{error}");

	// Direct calls to `plan_publish_rate_limits_for_requests` also ignore
	// requests whose `run_scope` is not `Publish`.
	let mut direct_requests = unscoped_requests.clone();
	let tags = monochange_publish::configured_release_tags(&configuration, &discovery.packages)
		.expect("tags:");
	monochange_publish::assign_publish_run_scopes(
		&mut direct_requests,
		&unscoped_requests,
		&tags,
		&primary_env,
	);
	let direct_report =
		plan_publish_rate_limits_for_requests(&direct_requests, RateLimitOperation::Publish, false);
	assert_eq!(direct_report.windows.len(), 1);
	assert_eq!(direct_report.windows[0].registry, RegistryKind::CratesIo);
	assert_eq!(direct_report.windows[0].pending, 1);
}
