use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::MockServer;
use monochange_cargo::extract_workspace_package_table;
use monochange_cargo::read_workspace_package_table;
use monochange_cargo::write_cargo_placeholder_manifest;
use monochange_core::DependencyKind;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishFlow;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::PublishTimeoutSettings;
use monochange_core::ReleaseRecord;
use monochange_core::SourceProvider;
use monochange_core::TrustedPublishingMode;
use monochange_core::TrustedPublishingSettings;
use monochange_core::VersionSource;
use monochange_github::GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN;
use monochange_github::GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL;
use monochange_github::GitHubTrustContext;
use monochange_github::json_value_contains;
use monochange_github::parse_github_workflow_ref;
use monochange_github::resolve_github_job_environment;
use monochange_github::trust_list_contains_context;
use monochange_npm::append_npm_trust_environment_arg;
use monochange_npm::build_npm_trust_command;
use monochange_publish::CommandExecutor;
use monochange_publish::CommandOutput;
use monochange_publish::CommandSpec;
use monochange_publish::PLACEHOLDER_VERSION;
use monochange_publish::ProcessCommandExecutor;
use monochange_publish::RegistryEndpoints;
use monochange_publish::append_publish_dry_run_args;
use monochange_publish::build_npm_placeholder_publish_command;
use monochange_publish::build_npm_release_publish_command;
use monochange_publish::build_publish_command;
use monochange_publish::crates_io_index_entry_path;
use monochange_publish::crates_io_index_version_exists;
use monochange_publish::default_registry_kind_for_ecosystem;
use monochange_publish::ensure_publish_report_succeeded;
use monochange_publish::filter_pending_publish_requests_with_transport;
use monochange_publish::forbidden_npm_token_env_keys;
use monochange_publish::publish_report_json_error;
use monochange_publish::registry_version_exists;
use monochange_publish::render_command;
use monochange_publish::render_command_error;
use monochange_publish::resolve_placeholder_readme;
use monochange_publish::resolve_registry_kind;
use monochange_publish::set_npm_publish_otp_for_requests;
use monochange_publish::write_publish_report_artifact;
use monochange_test_helpers::git;
use monochange_test_helpers::install_rustls_ring_provider;
use reqwest::Client;
use semver::Version;
use serde_json::Value as JsonValue;
use temp_env::with_vars;
use tempfile::TempDir;
use toml::Value as TomlValue;

use super::*;

async fn try_run_placeholder_publish_with_npm_otp(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	selected_packages: &BTreeSet<String>,
	dry_run: bool,
	npm_otp: Option<&str>,
	_quiet: bool,
) -> PackagePublishExecutionResult {
	try_run_placeholder_publish_with_progress(
		root,
		configuration,
		selected_packages,
		dry_run,
		npm_otp,
		&monochange_publish::NoopPublishProgressReporter,
	)
	.await
}
use crate::tests::TEST_ENV_LOCK;

fn build_placeholder_directory(
	root: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<TempDir> {
	monochange_publish::build_placeholder_directory(
		root,
		request,
		source,
		&placeholder_manifest_writer_registry(),
	)
}

#[allow(clippy::too_many_arguments)]
async fn execute_publish_requests(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
) -> MonochangeResult<PackagePublishReport> {
	monochange_publish::execute_publish_requests(
		root,
		source,
		mode,
		dry_run,
		requests,
		client,
		endpoints,
		env_map,
		executor,
		&build_publish_command_builder(),
		&placeholder_manifest_writer_registry(),
		&publish_readiness_registry(None),
		&CliPublishTrustHandler,
	)
	.await
}

fn enforce_release_attestation_prerequisites(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<()> {
	monochange_publish::enforce_release_attestation_prerequisites(
		request,
		env_map,
		&build_publish_command_builder(),
	)
}

struct FakeExecutor {
	outputs: VecDeque<CommandOutput>,
	commands: Vec<CommandSpec>,
}

impl FakeExecutor {
	fn new(outputs: Vec<CommandOutput>) -> Self {
		Self {
			outputs: VecDeque::from(outputs),
			commands: Vec::new(),
		}
	}
}

impl CommandExecutor for FakeExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		self.commands.push(spec.clone());
		self.outputs
			.pop_front()
			.ok_or_else(|| MonochangeError::Discovery("missing fake command output".to_string()))
	}
}

fn sample_request(registry: RegistryKind) -> PublishRequest {
	PublishRequest {
		package_id: "pkg".to_string(),

		package_name: if registry == RegistryKind::Jsr {
			"@scope/pkg".to_string()
		} else {
			"pkg".to_string()
		},

		ecosystem: if registry == RegistryKind::CratesIo {
			Ecosystem::Cargo
		} else if registry == RegistryKind::Npm {
			Ecosystem::Npm
		} else if registry == RegistryKind::PubDev {
			Ecosystem::Dart
		} else if registry == RegistryKind::Pypi {
			Ecosystem::Python
		} else if registry == RegistryKind::GoProxy {
			Ecosystem::Go
		} else {
			Ecosystem::Deno
		},

		manifest_path: PathBuf::from("/workspace/pkg/manifest"),
		package_root: PathBuf::from("/workspace/pkg"),
		registry,
		package_manager: (registry == RegistryKind::Npm).then(|| "npm".to_string()),
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		flow: PublishFlow::Direct,
		version: "1.2.3".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings {
			enabled: false,
			mode: TrustedPublishingMode::Required,
			repository: None,
			workflow: None,
			environment: None,
		},
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		placeholder_readme: "placeholder".to_string(),
		fail_on_duplicate: false,
	}
}

fn sample_publish_outcome(package: &str, status: PackagePublishStatus) -> PackagePublishOutcome {
	PackagePublishOutcome {
		package: package.to_string(),
		ecosystem: Ecosystem::Npm,
		registry: RegistryKind::Npm.to_string(),
		version: "1.2.3".to_string(),
		status,
		message: format!("{status:?}"),
		placeholder: false,
		trusted_publishing: disabled_trust_outcome(),
		command: None,
		stdout: None,
		stderr: None,
		rate_limit: None,
	}
}

fn sample_source() -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	}
}

fn trusted_request(registry: RegistryKind) -> PublishRequest {
	let mut request = sample_request(registry);
	request.trusted_publishing.enabled = true;
	request
}

fn trusted_provenance_request(registry: RegistryKind) -> PublishRequest {
	let mut request = trusted_request(registry);
	request.attestations.require_registry_provenance = true;
	request
}

fn github_oidc_env() -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_JOB".to_string(), "release".to_string()),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL.to_string(),
			"https://token.actions.githubusercontent.com".to_string(),
		),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN.to_string(),
			"request-token".to_string(),
		),
	])
}

fn sample_endpoints(base_url: &str) -> RegistryEndpoints {
	RegistryEndpoints {
		npm_registry: base_url.to_string(),
		crates_io_api: base_url.to_string(),
		crates_io_index: base_url.to_string(),
		pub_dev_api: base_url.to_string(),
		jsr_base: base_url.to_string(),
		pypi_api: base_url.to_string(),
		go_proxy: base_url.to_string(),
		cocoapods_trunk_api: base_url.to_string(),
		maven_central: base_url.to_string(),
	}
}

fn workflow_root() -> TempDir {
	let root = tempfile::tempdir().expect("tempdir:");
	let workflows = root.path().join(".github/workflows");
	fs::create_dir_all(&workflows).expect("mkdir:");
	fs::write(
		workflows.join("publish.yml"),
		"jobs:\n  release:\n    environment: publisher\n",
	)
	.expect("write workflow:");
	root
}

fn sample_configuration(
	packages: &[(&str, monochange_core::PackageType, bool)],
) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: PathBuf::from("/workspace"),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: packages
			.iter()
			.map(|(id, package_type, enabled)| {
				monochange_core::PackageDefinition {
					id: (*id).to_string(),
					path: PathBuf::from(id),
					package_type: *package_type,
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
					release: true,
					version_format: monochange_core::VersionFormat::Primary,
					version_source: VersionSource::default(),
					initial_version: None,
					bump_ceiling: None,
					classification_enforced: None,

					floating_tags: Vec::new(),
					publish: monochange_core::PublishSettings {
						enabled: *enabled,
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
	}
}

#[test]
fn trust_list_contains_context_supports_json_and_text() {
	let context = GitHubTrustContext {
		repository: "monochange/monochange".to_string(),
		workflow: "publish.yml".to_string(),
		environment: Some("publisher".to_string()),
	};
	assert!(trust_list_contains_context(
		r#"{"publisher":{"repository":"monochange/monochange","workflow":"publish.yml","environment":"publisher"}}"#,
		&context,
	));
	assert!(trust_list_contains_context(
		"repository monochange/monochange workflow publish.yml environment publisher",
		&context,
	));
}

#[test]
fn resolve_registry_kind_defaults_and_custom_errors_match_expectations() {
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Cargo).expect("cargo registry:"),
		RegistryKind::CratesIo
	);
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Npm).expect("npm registry:"),
		RegistryKind::Npm
	);
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Deno).expect("jsr registry:"),
		RegistryKind::Jsr
	);
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Dart).expect("pub registry:"),
		RegistryKind::PubDev
	);

	let error = resolve_registry_kind(
		Some(&PublishRegistry::Custom("internal".to_string())),
		Ecosystem::Npm,
	)
	.expect_err("expected custom registry error");
	assert!(
		error
			.to_string()
			.contains("does not support custom registry `internal`")
	);
	let unsupported = default_registry_kind_for_ecosystem("ruby")
		.expect_err("expected unsupported ecosystem error");
	assert!(
		unsupported
			.to_string()
			.contains("does not support ecosystem `ruby`")
	);
}

#[test]
fn build_placeholder_requests_skip_missing_or_disabled_packages_and_report_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let mut configuration =
		crate::load_workspace_configuration(root.path()).expect("configuration");
	let package = PackageRecord {
		id: "pkg".to_string(),
		name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: root.path().join("packages/pkg/package.json"),
		workspace_root: root.path().to_path_buf(),
		current_version: Some(Version::parse("1.0.0").expect("version")),
		publish_state: PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::from([
			("config_id".to_string(), "pkg".to_string()),
			("manager".to_string(), "pnpm".to_string()),
		]),
		declared_dependencies: Vec::new(),
	};

	let mut disabled = configuration.clone();
	disabled.packages[0].publish.enabled = false;
	disabled.packages.push(monochange_core::PackageDefinition {
		id: "missing".to_string(),
		..configuration.packages[0].clone()
	});
	let requests =
		build_placeholder_requests(root.path(), &disabled, &[package.clone()], &BTreeSet::new())
			.expect("requests");
	assert!(requests.is_empty());

	let requests = build_placeholder_requests(
		root.path(),
		&configuration,
		&[package.clone()],
		&BTreeSet::new(),
	)
	.expect("requests");
	assert_eq!(requests[0].package_manager.as_deref(), Some("pnpm"));

	let selected = build_placeholder_requests(
		root.path(),
		&configuration,
		&[package.clone()],
		&BTreeSet::from(["pkg".to_string()]),
	)
	.expect("selected requests");
	assert_eq!(selected.len(), 1);

	configuration.packages[0].publish.registry =
		Some(PublishRegistry::Custom("internal".to_string()));
	let registry_error = build_placeholder_requests(
		root.path(),
		&configuration,
		&[package.clone()],
		&BTreeSet::new(),
	)
	.expect_err("expected registry error");
	assert!(registry_error.to_string().contains("custom registry"));

	let mut missing_readme =
		crate::load_workspace_configuration(root.path()).expect("configuration");
	missing_readme.packages[0].publish.placeholder.readme_file = Some(PathBuf::from("missing.md"));
	let readme_error =
		build_placeholder_requests(root.path(), &missing_readme, &[package], &BTreeSet::new())
			.expect_err("expected readme error");
	assert!(
		readme_error
			.to_string()
			.contains("failed to read placeholder README")
	);
}

#[test]
fn build_publish_command_covers_all_supported_registries() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let npm_placeholder = build_publish_command(
		&sample_request(RegistryKind::Npm),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(
		npm_placeholder.args,
		vec![
			"publish".to_string(),
			tempdir.path().display().to_string(),
			"--access".to_string(),
			"public".to_string(),
		]
	);
	let npm = build_publish_command(
		&sample_request(RegistryKind::Npm),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(npm.program, "npm");
	let pnpm_request = PublishRequest {
		package_manager: Some("pnpm".to_string()),
		..sample_request(RegistryKind::Npm)
	};
	let pnpm_placeholder = build_publish_command(
		&pnpm_request,
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(pnpm_placeholder.program, "pnpm");

	let pnpm = build_publish_command(&pnpm_request, PackagePublishRunMode::Release, None, false);
	assert_eq!(pnpm.program, "pnpm");
	let trusted_pnpm_request = PublishRequest {
		trusted_publishing: TrustedPublishingSettings {
			enabled: true,
			mode: TrustedPublishingMode::Required,
			repository: None,
			workflow: None,
			environment: None,
		},
		..pnpm_request
	};
	let trusted_pnpm = build_publish_command(
		&trusted_pnpm_request,
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(trusted_pnpm.program, "pnpm");
	assert_eq!(
		trusted_pnpm.args.last().map(String::as_str),
		Some("--no-git-checks")
	);
	let cargo_placeholder = build_publish_command(
		&sample_request(RegistryKind::CratesIo),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(cargo_placeholder.program, "cargo");
	assert!(
		cargo_placeholder
			.args
			.contains(&tempdir.path().join("Cargo.toml").display().to_string())
	);
	let cargo = build_publish_command(
		&sample_request(RegistryKind::CratesIo),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(cargo.program, "cargo");
	let dart = build_publish_command(
		&sample_request(RegistryKind::PubDev),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(dart.program, "dart");
	let dart_placeholder = build_publish_command(
		&sample_request(RegistryKind::PubDev),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(dart_placeholder.cwd, tempdir.path());
	let flutter = build_publish_command(
		&PublishRequest {
			ecosystem: Ecosystem::Dart,
			package_metadata: BTreeMap::from([("is_flutter".to_string(), "true".to_string())]),
			..sample_request(RegistryKind::PubDev)
		},
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(flutter.program, "flutter");
	let pure_dart = build_publish_command(
		&PublishRequest {
			ecosystem: Ecosystem::Dart,
			..sample_request(RegistryKind::PubDev)
		},
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(pure_dart.program, "dart");
	let jsr = build_publish_command(
		&sample_request(RegistryKind::Jsr),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(jsr.program, "deno");
	assert_eq!(jsr.cwd, tempdir.path());
	let jsr_release = build_publish_command(
		&sample_request(RegistryKind::Jsr),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(jsr_release.cwd, PathBuf::from("/workspace/pkg"));
	let pypi_placeholder = build_publish_command(
		&sample_request(RegistryKind::Pypi),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(pypi_placeholder.program, "sh");
	assert_eq!(pypi_placeholder.cwd, tempdir.path());
	assert!(render_command(&pypi_placeholder).contains("uv publish --trusted-publishing never"));
	let pypi_release_request = PublishRequest {
		trusted_publishing: TrustedPublishingSettings {
			enabled: true,
			mode: TrustedPublishingMode::Required,
			..TrustedPublishingSettings::default()
		},
		..sample_request(RegistryKind::Pypi)
	};
	let pypi_release = build_publish_command(
		&pypi_release_request,
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(pypi_release.cwd, PathBuf::from("/workspace/pkg"));
	assert!(render_command(&pypi_release).contains("uv publish --trusted-publishing always"));

	let go_request = PublishRequest {
		ecosystem: Ecosystem::Go,
		package_name: "api".to_string(),
		package_root: PathBuf::from("/workspace/api"),
		package_metadata: BTreeMap::from([
			(
				"module_path".to_string(),
				"github.com/example/api".to_string(),
			),
			("relative_path".to_string(), "api".to_string()),
		]),
		..sample_request(RegistryKind::GoProxy)
	};
	let go = build_publish_command(&go_request, PackagePublishRunMode::Release, None, false);
	assert_eq!(go.program, "git");
	assert_eq!(go.args, vec!["tag".to_string(), "api/v1.2.3".to_string()]);
}

#[test]
fn build_placeholder_directory_writes_go_mod_for_go_proxy() {
	let root = tempfile::tempdir().expect("tempdir");
	let request = PublishRequest {
		package_metadata: BTreeMap::from([(
			"module_path".to_string(),
			"github.com/example/repo/api".to_string(),
		)]),
		..sample_request(RegistryKind::GoProxy)
	};

	let placeholder =
		build_placeholder_directory(root.path(), &request, None).expect("go placeholder directory");
	let go_mod = fs::read_to_string(placeholder.path().join("go.mod")).expect("go.mod");

	assert_eq!(go_mod, "module github.com/example/repo/api\n\ngo 1.22\n");
}

#[test]
fn build_publish_command_appends_dry_run_flags_for_supported_registries() {
	let tempdir = tempfile::tempdir().expect("tempdir:");

	let npm = build_publish_command(
		&sample_request(RegistryKind::Npm),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert_eq!(npm.args.last(), Some(&"--dry-run".to_string()));

	let cargo = build_publish_command(
		&sample_request(RegistryKind::CratesIo),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert_eq!(cargo.args.last(), Some(&"--dry-run".to_string()));

	let dart = build_publish_command(
		&sample_request(RegistryKind::PubDev),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert!(dart.args.contains(&"--dry-run".to_string()));
	assert!(!dart.args.contains(&"--force".to_string()));

	let jsr = build_publish_command(
		&sample_request(RegistryKind::Jsr),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert_eq!(jsr.args.last(), Some(&"--dry-run".to_string()));

	let pypi = build_publish_command(
		&sample_request(RegistryKind::Pypi),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert!(!render_command(&pypi).contains("--dry-run"));

	let go = build_publish_command(
		&sample_request(RegistryKind::GoProxy),
		PackagePublishRunMode::Release,
		None,
		true,
	);
	assert!(!go.args.contains(&"--dry-run".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn registry_version_exists_parses_all_supported_registry_shapes() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": {
				"1.2.3": { "name": "pkg" }
			}
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "num": "1.2.3" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/packages/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "version": "1.2.3" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/@scope/pkg/meta.json");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": { "1.2.3": {} }
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/pkg/json");
		then.status(200).json_body_obj(&serde_json::json!({
			"releases": { "1.2.3": [] }
		}));
	});
	server.mock(|when, then| {
		when.method(GET)
			.path("/github.com/example/!repo/api/@v/v1.2.3.info");
		then.status(200).json_body_obj(&serde_json::json!({
			"Version": "v1.2.3",
			"Time": "2026-04-28T00:00:00Z"
		}));
	});
	install_rustls_ring_provider();

	let client = Client::builder().build().expect("http client:");
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
		cocoapods_trunk_api: server.base_url(),
		maven_central: server.base_url(),
	};

	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::Npm))
			.await
			.expect("npm exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::CratesIo))
			.await
			.expect("cargo exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::PubDev))
			.await
			.expect("dart exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::Jsr))
			.await
			.expect("jsr exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::Pypi))
			.await
			.expect("PyPI exists:")
	);
	let go_request = PublishRequest {
		package_metadata: BTreeMap::from([(
			"module_path".to_string(),
			"github.com/example/Repo/api".to_string(),
		)]),
		..sample_request(RegistryKind::GoProxy)
	};
	assert!(
		registry_version_exists(&client, &endpoints, &go_request)
			.await
			.expect("Go exists:")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn crates_io_index_version_exists_handles_missing_invalid_and_nonmatching_entries() {
	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");

	let missing_server = MockServer::start();
	missing_server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(404);
	});
	assert!(
		!crates_io_index_version_exists(
			&client,
			&sample_endpoints(&missing_server.base_url()),
			&sample_request(RegistryKind::CratesIo),
		)
		.await
		.expect("missing index entry:")
	);

	let invalid_server = MockServer::start();
	invalid_server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(200).body("not-json\n");
	});
	let invalid_error = crates_io_index_version_exists(
		&client,
		&sample_endpoints(&invalid_server.base_url()),
		&sample_request(RegistryKind::CratesIo),
	)
	.await
	.expect_err("expected index decode error");
	assert!(
		invalid_error
			.to_string()
			.contains("crates.io index decode failed")
	);

	let nonmatching_server = MockServer::start();
	nonmatching_server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(200)
			.body("{\"name\":\"pkg\"}\n{\"name\":\"pkg\",\"vers\":\"9.9.9\"}\n");
	});
	assert!(
		!crates_io_index_version_exists(
			&client,
			&sample_endpoints(&nonmatching_server.base_url()),
			&sample_request(RegistryKind::CratesIo),
		)
		.await
		.expect("nonmatching index entry:")
	);
}

#[test]
fn write_cargo_placeholder_manifest_reports_manifest_io_parse_and_copy_failures() {
	let root = tempfile::tempdir().expect("tempdir");
	let dir = tempfile::tempdir().expect("tempdir");

	let missing_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: root.path().join("missing/Cargo.toml"),
			package_root: PathBuf::from("missing"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected missing manifest error");
	assert!(
		missing_error
			.to_string()
			.contains("failed to read Cargo manifest")
	);

	let invalid_manifest = root.path().join("invalid/Cargo.toml");
	fs::create_dir_all(invalid_manifest.parent().expect("parent")).expect("mkdir");
	fs::write(&invalid_manifest, "[package").expect("write invalid manifest");
	let parse_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: invalid_manifest,
			package_root: PathBuf::from("invalid"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected parse error");
	assert!(parse_error.to_string().contains("failed to parse"));

	let missing_package_manifest = root.path().join("no-package/Cargo.toml");
	fs::create_dir_all(missing_package_manifest.parent().expect("parent")).expect("mkdir");
	fs::write(&missing_package_manifest, "[workspace]\nmembers = []\n")
		.expect("write workspace manifest");
	let missing_package_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: missing_package_manifest,
			package_root: PathBuf::from("no-package"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected missing package error");
	assert!(
		missing_package_error
			.to_string()
			.contains("is missing [package]")
	);

	let copy_manifest = root.path().join("copy/Cargo.toml");
	fs::create_dir_all(copy_manifest.parent().expect("parent")).expect("mkdir");
	fs::write(
		&copy_manifest,
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license-file = \"LICENSE.md\"\n",
		),
	)
	.expect("write manifest");
	let copy_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: copy_manifest,
			package_root: PathBuf::from("copy"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected copy error");
	assert!(
		copy_error
			.to_string()
			.contains("failed to copy placeholder license file")
	);
}

#[test]
fn cargo_and_jsr_placeholder_manifests_report_directory_write_failures() {
	let tempdir = tempfile::tempdir().expect("tempdir");
	let package_root = tempdir.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(
		&manifest_path,
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license = \"MIT\"\n",
		),
	)
	.expect("write manifest");
	let request = PublishRequest {
		manifest_path,
		package_root,
		..sample_request(RegistryKind::CratesIo)
	};

	let file_root = tempdir.path().join("file-root");
	fs::write(&file_root, "file").expect("write file root");
	let mkdir_error = write_cargo_placeholder_manifest(&file_root, &request, tempdir.path(), None)
		.expect_err("expected src dir error");
	assert!(
		mkdir_error
			.to_string()
			.contains("failed to create placeholder src directory")
	);

	let src_file_root = tempdir.path().join("src-file-root");
	fs::create_dir_all(src_file_root.join("src/lib.rs")).expect("create lib.rs directory");
	let src_write_error =
		write_cargo_placeholder_manifest(&src_file_root, &request, tempdir.path(), None)
			.expect_err("expected src file write error");
	assert!(
		src_write_error
			.to_string()
			.contains("failed to write placeholder src/lib.rs")
	);

	let manifest_file_root = tempdir.path().join("manifest-file-root");
	fs::create_dir_all(manifest_file_root.join("src")).expect("create src");
	fs::create_dir_all(manifest_file_root.join("Cargo.toml")).expect("create Cargo.toml directory");
	let manifest_write_error =
		write_cargo_placeholder_manifest(&manifest_file_root, &request, tempdir.path(), None)
			.expect_err("expected cargo manifest write error");
	assert!(
		manifest_write_error
			.to_string()
			.contains("failed to write placeholder Cargo.toml")
	);

	let jsr_mod_root = tempdir.path().join("jsr-mod-root");
	fs::create_dir_all(&jsr_mod_root).expect("mkdir jsr root");
	fs::create_dir_all(jsr_mod_root.join("mod.ts")).expect("create mod.ts directory");
	let jsr_mod_error =
		write_jsr_placeholder_manifest(&jsr_mod_root, &sample_request(RegistryKind::Jsr), None)
			.expect_err("expected mod.ts write error");
	assert!(
		jsr_mod_error
			.to_string()
			.contains("failed to write placeholder mod.ts")
	);
}

fn sample_npm_package_with_dependencies(
	id: &str,
	name: &str,
	declared_dependencies: Vec<monochange_core::PackageDependency>,
) -> PackageRecord {
	PackageRecord {
		id: format!("npm:packages/{id}/package.json"),
		name: name.to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from(format!("/workspace/packages/{id}/package.json")),
		workspace_root: PathBuf::from("/workspace"),
		current_version: Some(Version::parse("1.0.0").expect("version")),
		publish_state: PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::from([
			("config_id".to_string(), id.to_string()),
			("manager".to_string(), "pnpm".to_string()),
		]),
		declared_dependencies,
	}
}

fn sample_npm_dependency(name: &str, kind: DependencyKind) -> monochange_core::PackageDependency {
	monochange_core::PackageDependency {
		name: name.to_string(),
		kind,
		version_constraint: Some("workspace:*".to_string()),
		optional: false,
		source_field: Some(
			match kind {
				DependencyKind::Development => "devDependencies",
				DependencyKind::Peer => "peerDependencies",
				_ => "dependencies",
			}
			.to_string(),
		),
	}
}

fn sample_npm_publication(package: &str) -> PackagePublicationTarget {
	PackagePublicationTarget {
		package: package.to_string(),
		ecosystem: Ecosystem::Npm,
		registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
		version: "1.2.3".to_string(),
		mode: PublishMode::Builtin,
		flow: monochange_core::PublishFlow::default(),
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
	}
}

#[test]
fn build_release_requests_reports_development_dependency_cycles() {
	let configuration = sample_configuration(&[
		("core", monochange_core::PackageType::Npm, true),
		("utils", monochange_core::PackageType::Npm, true),
	]);
	let packages = vec![
		sample_npm_package_with_dependencies(
			"core",
			"core",
			vec![sample_npm_dependency("utils", DependencyKind::Development)],
		),
		sample_npm_package_with_dependencies(
			"utils",
			"utils",
			vec![sample_npm_dependency("core", DependencyKind::Development)],
		),
	];
	let publications = vec![
		sample_npm_publication("utils"),
		sample_npm_publication("core"),
	];

	let error = build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
		.expect_err("development dependency cycles should fail");
	let message = error.to_string();

	assert!(message.contains("cyclic publish dependencies detected"));
	assert!(message.contains("core -> utils"));
	assert!(message.contains("utils -> core"));
}

#[test]
fn write_placeholder_directory_builds_npm_jsr_dart_and_python_scaffolds() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let npm = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::Npm),
		Some(&sample_source()),
	)
	.expect("npm placeholder:");
	assert!(npm.path().join("package.json").is_file());

	let dart = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::PubDev),
		Some(&sample_source()),
	)
	.expect("dart placeholder:");
	assert!(dart.path().join("pubspec.yaml").is_file());

	let jsr = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::Jsr),
		Some(&sample_source()),
	)
	.expect("jsr placeholder:");
	assert!(jsr.path().join("deno.json").is_file());

	let python_request = PublishRequest {
		package_name: "Example-Pkg.Name".to_string(),
		..sample_request(RegistryKind::Pypi)
	};
	let python =
		build_placeholder_directory(tempdir.path(), &python_request, Some(&sample_source()))
			.expect("Python placeholder:");
	let pyproject =
		fs::read_to_string(python.path().join("pyproject.toml")).expect("read pyproject.toml");
	assert!(pyproject.contains("name = \"Example-Pkg.Name\""));
	assert!(pyproject.contains("packages = [\"src/example_pkg_name\"]"));
	assert!(
		python
			.path()
			.join("src")
			.join("example_pkg_name")
			.join("__init__.py")
			.is_file()
	);

	let digit_request = PublishRequest {
		package_name: "123-pkg".to_string(),
		..sample_request(RegistryKind::Pypi)
	};
	let digit_python = build_placeholder_directory(tempdir.path(), &digit_request, None)
		.expect("digit Python placeholder:");
	assert!(
		digit_python
			.path()
			.join("src")
			.join("placeholder_123_pkg")
			.join("__init__.py")
			.is_file()
	);
}

#[test]
fn python_placeholder_manifest_writers_report_io_errors() {
	let request = sample_request(RegistryKind::Pypi);
	let tempdir = tempfile::tempdir().expect("tempdir:");
	fs::create_dir(tempdir.path().join("pyproject.toml")).expect("create pyproject dir");
	let error = write_python_placeholder_manifest(tempdir.path(), &request, None)
		.expect_err("pyproject write should fail");
	assert!(
		error
			.to_string()
			.contains("failed to write placeholder pyproject.toml")
	);

	let tempdir = tempfile::tempdir().expect("tempdir:");
	fs::write(tempdir.path().join("src"), "not a directory").expect("write src file");
	let error = write_python_placeholder_manifest(tempdir.path(), &request, None)
		.expect_err("package directory create should fail");
	assert!(
		error
			.to_string()
			.contains("failed to create placeholder Python package")
	);

	let tempdir = tempfile::tempdir().expect("tempdir:");
	let module_dir = tempdir.path().join("src").join("pkg");
	fs::create_dir_all(&module_dir).expect("create module dir");
	fs::create_dir(module_dir.join("__init__.py")).expect("create init dir");
	let error = write_python_placeholder_manifest(tempdir.path(), &request, None)
		.expect_err("module write should fail");
	assert!(
		error
			.to_string()
			.contains("failed to write placeholder Python package module")
	);
}

#[test]
fn planned_and_skip_trust_outcomes_cover_npm_and_manual_flows() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let planned = planned_trust_outcome(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(planned.status, TrustedPublishingStatus::Planned);
	assert_eq!(planned.environment, Some("publisher".to_string()));
	assert_eq!(
		planned.setup_url.as_deref(),
		Some("https://www.npmjs.com/package/pkg/access")
	);

	let skipped = trust_outcome_for_skip(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(skipped.status, TrustedPublishingStatus::Configured);
	assert_eq!(
		skipped.setup_url.as_deref(),
		Some("https://www.npmjs.com/package/pkg/access")
	);

	let manual = planned_trust_outcome(
		&trusted_request(RegistryKind::CratesIo),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(manual.status, TrustedPublishingStatus::ManualActionRequired);
	assert_eq!(manual.repository.as_deref(), Some("monochange/monochange"));
	assert_eq!(manual.workflow.as_deref(), Some("publish.yml"));
	assert_eq!(manual.environment.as_deref(), Some("publisher"));
	assert!(
		manual
			.setup_url
			.expect("expected setup url")
			.contains("crates.io/crates/pkg")
	);
}

#[test]
fn enforce_release_trust_prerequisites_accepts_configured_github_oidc_contexts() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL.to_string(),
			"https://token.actions.githubusercontent.com".to_string(),
		),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN.to_string(),
			"request-token".to_string(),
		),
	]);

	enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect("expected npm trust prereq success:");

	enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::CratesIo),
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect("expected crates.io trust prereq success:");

	enforce_release_trust_prerequisites(
		&sample_request(RegistryKind::Npm),
		None,
		root.path(),
		&BTreeMap::new(),
	)
	.expect("expected disabled trust success:");

	let mut mismatched_workflow_request = trusted_request(RegistryKind::PubDev);
	mismatched_workflow_request.trusted_publishing.workflow = Some("release.yml".to_string());
	let mismatched_context_error = enforce_release_trust_prerequisites(
		&mismatched_workflow_request,
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect_err("expected mismatched context error");
	assert!(
		mismatched_context_error
			.to_string()
			.contains("expected GitHub workflow `release.yml`, but detected `publish.yml`")
	);
}

#[test]
fn enforce_release_attestation_prerequisites_accepts_supported_registry_provenance() {
	let env_map = github_oidc_env();

	enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Npm),
		&env_map,
	)
	.expect("expected npm provenance policy success");

	enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Jsr),
		&env_map,
	)
	.expect("expected JSR provenance policy success");
}

#[test]
fn enforce_release_attestation_prerequisites_rejects_unsupported_registry_provenance() {
	let error = enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::CratesIo),
		&github_oidc_env(),
	)
	.expect_err("crates.io should reject registry provenance policy");

	let message = error.to_string();
	assert!(message.contains("cannot require registry-native package provenance"));
	assert!(message.contains("registry-native provenance is not available"));

	let error = enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Pypi),
		&github_oidc_env(),
	)
	.expect_err("PyPI should reject until the built-in publisher can require attestations");

	let message = error.to_string();
	assert!(message.contains("registry supports provenance"));
	assert!(message.contains("built-in publisher"));
}

#[test]
fn enforce_release_trust_prerequisites_rejects_unsupported_provider_registry_pairs() {
	let root = workflow_root();
	let circle_env = BTreeMap::from([
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
	]);
	let error = enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&circle_env,
	)
	.expect_err("CircleCI npm trusted publishing should be rejected");
	let message = error.to_string();
	assert!(message.contains("cannot enforce trusted publishing"));
	assert!(message.contains("CircleCI"));

	let gitlab_env = BTreeMap::from([
		("GITLAB_CI".to_string(), "true".to_string()),
		(
			"CI_PROJECT_PATH".to_string(),
			"monochange/monochange".to_string(),
		),
		("CI_JOB_ID".to_string(), "42".to_string()),
	]);
	enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&gitlab_env,
	)
	.expect("supported non-GitHub trusted publishing identities should pass capability checks");
}

#[test]
fn verify_github_trust_context_reports_identity_mismatches() {
	let root = workflow_root();
	let request = trusted_request(RegistryKind::Npm);
	let expected = GitHubTrustContext {
		repository: "monochange/monochange".to_string(),
		workflow: "publish.yml".to_string(),
		environment: Some("publisher".to_string()),
	};

	let missing_repository = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		None,
		Some("publish.yml"),
		Some("publisher"),
	)
	.expect_err("missing GitHub repository should fail");
	assert!(
		missing_repository
			.to_string()
			.contains("GitHub Actions did not expose `GITHUB_REPOSITORY`")
	);

	let repository_mismatch = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		Some("other/repo"),
		Some("publish.yml"),
		Some("publisher"),
	)
	.expect_err("mismatched GitHub repository should fail");
	assert!(
		repository_mismatch.to_string().contains(
			"expected GitHub repository `monochange/monochange`, but detected `other/repo`"
		)
	);

	let missing_workflow = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		Some("monochange/monochange"),
		None,
		Some("publisher"),
	)
	.expect_err("missing GitHub workflow should fail");
	assert!(
		missing_workflow
			.to_string()
			.contains("GitHub Actions did not expose `GITHUB_WORKFLOW_REF`")
	);

	let environment_mismatch = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		Some("monochange/monochange"),
		Some("publish.yml"),
		None,
	)
	.expect_err("missing GitHub environment should fail");
	assert!(
		environment_mismatch
			.to_string()
			.contains("expected GitHub environment `publisher`, but detected `none`")
	);

	let missing_oidc = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&GitHubTrustContext {
			environment: None,
			..expected
		},
		Some("monochange/monochange"),
		Some("publish.yml"),
		None,
	)
	.expect_err("missing GitHub OIDC token request variables should fail");
	assert!(missing_oidc.to_string().contains("grant `id-token: write`"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_blocks_trusted_publish_before_external_command() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut executor = FakeExecutor::new(Vec::new());
	let error = execute_publish_requests(
		Path::new("."),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		false,
		&[trusted_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect_err("trusted publishing should block local release publish");

	assert!(
		error
			.to_string()
			.contains("local/manual publishing is not allowed")
	);
	assert!(executor.commands.is_empty());
}

#[test]
fn publish_report_artifact_round_trips_and_reports_io_errors() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_publish_outcome(
			"done",
			PackagePublishStatus::SkippedExisting,
		)],
	};
	let output = tempdir.path().join("nested/publish-result.json");

	write_publish_report_artifact(&output, &report)
		.unwrap_or_else(|error| panic!("write report: {error}"));
	let read_report = read_publish_report_artifact(&output)
		.unwrap_or_else(|error| panic!("read report: {error}"));
	assert_eq!(read_report, report);

	let missing_error = read_publish_report_artifact(&tempdir.path().join("missing.json"))
		.expect_err("missing artifact should fail");
	assert!(missing_error.to_string().contains("failed to read"));

	let invalid_json_path = tempdir.path().join("invalid.json");
	fs::write(&invalid_json_path, "not json")
		.unwrap_or_else(|error| panic!("write invalid json: {error}"));
	let parse_error =
		read_publish_report_artifact(&invalid_json_path).expect_err("invalid artifact should fail");
	assert!(parse_error.to_string().contains("failed to parse"));

	let write_error = write_publish_report_artifact(tempdir.path(), &report)
		.expect_err("directory output path should fail");
	assert!(write_error.to_string().contains("failed to write"));

	let parent_file = tempdir.path().join("file-parent");
	fs::write(&parent_file, "not a directory")
		.unwrap_or_else(|error| panic!("write file parent: {error}"));
	let create_error = write_publish_report_artifact(&parent_file.join("result.json"), &report)
		.expect_err("file parent should fail directory creation");
	assert!(create_error.to_string().contains("failed to create"));
	assert!(
		publish_report_json_error("bad json")
			.to_string()
			.contains("failed to serialize package publish report")
	);
}

fn write_cargo_manifest(root: &Path, contents: &str) -> PathBuf {
	let package_root = root.join("pkg");
	fs::create_dir_all(&package_root).expect("package dir");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(&manifest_path, contents).expect("write Cargo manifest");
	manifest_path
}

fn sample_cargo_request(root: &Path, manifest_path: &Path) -> PublishRequest {
	PublishRequest {
		manifest_path: manifest_path.to_path_buf(),
		package_root: root.join("pkg"),
		..sample_request(RegistryKind::CratesIo)
	}
}

#[test]
fn cargo_publish_readiness_blockers_report_manifest_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	let missing = root.path().join("pkg/Cargo.toml");
	let missing_request = sample_cargo_request(root.path(), &missing);
	let missing_error = cargo_publish_readiness_blockers(root.path(), &missing_request)
		.expect_err("expected read error");
	assert!(
		missing_error
			.to_string()
			.contains("failed to read Cargo manifest")
	);

	let invalid = write_cargo_manifest(root.path(), "not valid toml");
	let invalid_request = sample_cargo_request(root.path(), &invalid);
	let invalid_error = cargo_publish_readiness_blockers(root.path(), &invalid_request)
		.expect_err("expected parse error");
	assert!(invalid_error.to_string().contains("failed to parse"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_skips_external_and_existing_versions() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": { "1.2.3": {} }
		}));
	});
	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
		cocoapods_trunk_api: server.base_url(),
		maven_central: server.base_url(),
	};
	let request = PublishRequest {
		mode: PublishMode::External,
		flow: monochange_core::PublishFlow::Direct,
		..sample_request(RegistryKind::Npm)
	};
	let existing = sample_request(RegistryKind::Npm);
	let mut executor = FakeExecutor::new(Vec::new());

	let report = execute_publish_requests(
		Path::new("."),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		true,
		&[request, existing],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect("report:");
	assert_eq!(report.packages.len(), 2);
	assert_eq!(
		report.packages[0].status,
		PackagePublishStatus::SkippedExternal
	);
	assert_eq!(
		report.packages[1].status,
		PackagePublishStatus::SkippedExisting
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_reports_staged_outcome_for_staged_flow() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let root = workflow_root();
	let mut request = sample_request(RegistryKind::Npm);
	request.flow = PublishFlow::Staged;

	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: String::new(),
		stderr: String::new(),
	}]);

	let report = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		false,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect("report:");

	assert_eq!(report.packages.len(), 1);
	assert_eq!(report.packages[0].status, PackagePublishStatus::Staged);
	assert!(
		report.packages[0]
			.message
			.contains("npm stage approve <stage-id>")
	);
	assert_eq!(
		executor.commands[0]
			.args
			.iter()
			.map(String::as_str)
			.collect::<Vec<_>>(),
		vec!["stage", "publish", "--access", "public"]
	);
	assert_eq!(report.summary().staged, 1);
	assert_eq!(report.summary().published, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn release_dry_run_orders_cargo_dev_and_build_dependencies_before_dependents() {
	let expected_order = vec![
		"zephyr-build",
		"lima-dev",
		"quartz-build",
		"ember-dev",
		"nyx-build",
		"cedar-dev",
		"violet-build",
		"osprey-build",
		"delta-dev",
		"amber-dev",
		"mango-build",
		"sable-dev",
		"orbit-build",
		"binary-build",
		"atlas-dev",
		"config",
	];
	let packages = [
		("config", Some(("build-dependencies", "atlas-dev"))),
		("atlas-dev", Some(("build-dependencies", "binary-build"))),
		("binary-build", Some(("dev-dependencies", "orbit-build"))),
		("orbit-build", Some(("build-dependencies", "sable-dev"))),
		("sable-dev", Some(("dev-dependencies", "mango-build"))),
		("mango-build", Some(("build-dependencies", "amber-dev"))),
		("amber-dev", Some(("dev-dependencies", "delta-dev"))),
		("delta-dev", Some(("build-dependencies", "osprey-build"))),
		("osprey-build", Some(("dev-dependencies", "violet-build"))),
		("violet-build", Some(("build-dependencies", "cedar-dev"))),
		("cedar-dev", Some(("dev-dependencies", "nyx-build"))),
		("nyx-build", Some(("build-dependencies", "ember-dev"))),
		("ember-dev", Some(("dev-dependencies", "quartz-build"))),
		("quartz-build", Some(("build-dependencies", "lima-dev"))),
		("lima-dev", Some(("dev-dependencies", "zephyr-build"))),
		("zephyr-build", None),
	];
	let server = MockServer::start();

	for (package, _) in packages {
		server.mock(|when, then| {
			when.method(GET).path(format!("/crates/{package}"));
			then.status(404);
		});
	}

	let root = tempfile::tempdir().expect("tempdir:");
	let members = packages
		.iter()
		.map(|(package, _)| format!("\"{package}\""))
		.collect::<Vec<_>>()
		.join(", ");
	fs::write(
		root.path().join("Cargo.toml"),
		format!("[workspace]\nmembers = [{members}]\nresolver = \"2\"\n"),
	)
	.expect("write workspace manifest:");

	for (package, dependency) in packages {
		let package_dir = root.path().join(package);
		fs::create_dir_all(package_dir.join("src")).expect("mkdir package:");
		fs::write(package_dir.join("src/lib.rs"), "").expect("write package lib:");

		let dependency_section = dependency.map_or_else(String::new, |(kind, dependency)| {
			format!(
				"\n[{kind}]\n{dependency} = {{ path = \"../{dependency}\", version = \"1.0.0\" }}\n"
			)
		});
		fs::write(
			package_dir.join("Cargo.toml"),
			format!(
				concat!(
					"[package]\n",
					"name = \"{package}\"\n",
					"version = \"1.0.0\"\n",
					"edition = \"2021\"\n",
					"license = \"MIT\"\n",
					"description = \"test package\"\n",
					"{dependency_section}"
				),
				package = package,
				dependency_section = dependency_section,
			),
		)
		.expect("write package manifest:");
	}

	let configuration_packages = packages
		.iter()
		.map(|(package, _)| (*package, monochange_core::PackageType::Cargo, true))
		.collect::<Vec<_>>();
	let configuration = sample_configuration(&configuration_packages);
	let publications = packages
		.into_iter()
		.map(|(package, _)| {
			PackagePublicationTarget {
				package: package.to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: "1.0.0".to_string(),
				mode: PublishMode::Builtin,
				flow: monochange_core::PublishFlow::default(),
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
				timeout: PublishTimeoutSettings::default(),
				fail_on_duplicate: false,
			}
		})
		.collect::<Vec<_>>();

	temp_env::async_with_vars(
		[("MONOCHANGE_CRATES_IO_API_URL", Some(server.base_url()))],
		async {
			let report = run_publish_packages_with_publications(
				root.path(),
				&configuration,
				&publications,
				&BTreeSet::new(),
				true,
				false,
			)
			.await
			.expect("publish report:");
			let order = report
				.packages
				.iter()
				.map(|package| package.package.as_str())
				.collect::<Vec<_>>();

			assert_eq!(order, expected_order);
		},
	)
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_placeholder_dry_run_surfaces_manifest_prerequisites() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	let package_root = root.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	fs::write(
		package_root.join("Cargo.toml"),
		concat!("[package]\n", "name = \"pkg\"\n", "version = \"1.0.0\"\n",),
	)
	.expect("write manifest:");

	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut request = sample_request(RegistryKind::CratesIo);
	request.manifest_path = package_root.join("Cargo.toml");
	request.package_root = package_root;
	request.placeholder = true;
	let mut executor = FakeExecutor::new(Vec::new());

	let error = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Placeholder,
		true,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect_err("expected placeholder manifest error");
	assert!(
		error.to_string().contains(
			"placeholder publishing requires `package.license` or `package.license-file`"
		)
	);
	assert!(executor.commands.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_publishes_placeholder_and_flags_manual_trust() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	let package_root = root.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	fs::write(
		package_root.join("Cargo.toml"),
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license = \"MIT\"\n",
		),
	)
	.expect("write manifest:");

	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut request = trusted_request(RegistryKind::CratesIo);
	request.manifest_path = package_root.join("Cargo.toml");
	request.package_root = package_root;
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: String::new(),
		stderr: String::new(),
	}]);

	let report = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Placeholder,
		false,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect("report:");

	assert_eq!(report.packages[0].status, PackagePublishStatus::Published);
	assert_eq!(
		report.packages[0].trusted_publishing.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert_eq!(executor.commands[0].program, "cargo");
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_publish_requests_surfaces_publish_command_failures() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	install_rustls_ring_provider();
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: false,
		stdout: String::new(),
		stderr: "boom".to_string(),
	}]);

	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[sample_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect("publish report");

	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(report.packages[0].message.contains("npm publish"));
	assert!(report.packages[0].message.contains("boom"));

	let mut executor = FakeExecutor::new(Vec::new());
	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[sample_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.await
	.expect("publish report");
	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(
		report.packages[0]
			.message
			.contains("missing fake command output")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_placeholder_publish_uses_env_overrides_for_registry_endpoints() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});

	let root = tempfile::tempdir().expect("tempdir:");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("config:");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir:");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("manifest:");
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration:");

	temp_env::async_with_vars(
		vec![(
			"MONOCHANGE_NPM_REGISTRY_URL",
			Some(server.base_url().as_str()),
		)],
		async {
			let report = try_run_placeholder_publish_with_npm_otp(
				root.path(),
				&configuration,
				&BTreeSet::new(),
				true,
				Some("123456"),
				true,
			)
			.await
			.expect("placeholder report:");
			assert_eq!(report.mode, PackagePublishRunMode::Placeholder);
			assert_eq!(report.packages.len(), 1);
			assert_eq!(report.packages[0].status, PackagePublishStatus::Planned);
		},
	)
	.await;
}

#[test]
fn process_command_executor_runs_commands_and_reports_spawn_failures() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let command = CommandSpec {
		program: "sh".to_string(),
		args: vec![
			"-c".to_string(),
			"printf stdout; printf stderr >&2".to_string(),
		],
		cwd: tempdir.path().to_path_buf(),
		env: BTreeMap::new(),
		timeout: None,
	};

	let mut captured_executor = ProcessCommandExecutor::new(false);
	let captured = captured_executor
		.run(&command)
		.expect("expected captured command success:");
	assert!(captured.success);
	assert_eq!(captured.stdout, "stdout");
	assert_eq!(captured.stderr, "stderr");

	let mut streaming_executor = ProcessCommandExecutor::new(true);
	let streamed = streaming_executor
		.run(&command)
		.expect("expected streamed command success:");
	assert!(streamed.success);
	assert_eq!(streamed.stdout, "stdout");
	assert_eq!(streamed.stderr, "stderr");

	let mut marked_command = command.clone();
	marked_command.env.insert(
		"MONOCHANGE_PUBLISH_STREAM_OUTPUT".to_string(),
		"true".to_string(),
	);
	let marked_streamed = captured_executor
		.run(&marked_command)
		.expect("expected marked command to stream successfully:");
	assert!(marked_streamed.success);
	assert_eq!(marked_streamed.stdout, "stdout");
	assert_eq!(marked_streamed.stderr, "stderr");

	let error = captured_executor
		.run(&CommandSpec {
			program: "definitely-not-a-real-command".to_string(),
			args: Vec::new(),
			cwd: tempdir.path().to_path_buf(),
			env: BTreeMap::new(),
			timeout: None,
		})
		.expect_err("expected command failure");
	assert!(
		error
			.to_string()
			.contains("failed to run `definitely-not-a-real-command`")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_placeholder_publish_maps_build_request_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let mut configuration =
		crate::load_workspace_configuration(root.path()).expect("configuration");
	configuration.packages[0].publish.registry =
		Some(PublishRegistry::Custom("internal".to_string()));

	let error = try_run_placeholder_publish_with_npm_otp(
		root.path(),
		&configuration,
		&BTreeSet::new(),
		true,
		None,
		true,
	)
	.await
	.expect_err("custom registry should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Placeholder);
	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_placeholder_publish_maps_discovery_errors_via_malformed_config() {
	let valid_root = tempfile::tempdir().expect("tempdir");
	fs::write(
		valid_root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(valid_root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		valid_root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let configuration =
		crate::load_workspace_configuration(valid_root.path()).expect("configuration");

	let malformed_root = tempfile::tempdir().expect("tempdir");
	fs::write(
		malformed_root.path().join("monochange.toml"),
		"not valid toml [[[",
	)
	.expect("write malformed config");

	let error = try_run_placeholder_publish_with_npm_otp(
		malformed_root.path(),
		&configuration,
		&BTreeSet::new(),
		true,
		None,
		true,
	)
	.await
	.expect_err("malformed config should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Placeholder);
	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_publish_packages_maps_publish_all_discovery_errors_via_malformed_config() {
	let valid_root = tempfile::tempdir().expect("tempdir");
	fs::write(
		valid_root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(valid_root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		valid_root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let configuration =
		crate::load_workspace_configuration(valid_root.path()).expect("configuration");

	let malformed_root = tempfile::tempdir().expect("tempdir");
	fs::write(
		malformed_root.path().join("monochange.toml"),
		"not valid toml [[[",
	)
	.expect("write malformed config");

	let error = try_run_publish_packages_with_resume(
		malformed_root.path(),
		&configuration,
		None,
		&BTreeSet::new(),
		&BTreeSet::new(),
		&BTreeSet::new(),
		PublishPackagesOptions {
			publish_all_configured_packages: true,
			..PublishPackagesOptions::default()
		},
	)
	.await
	.expect_err("publish-all malformed config should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Release);
	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_publish_packages_with_publications_maps_discovery_errors_via_malformed_config() {
	let valid_root = tempfile::tempdir().expect("tempdir");
	fs::write(
		valid_root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(valid_root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		valid_root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let configuration =
		crate::load_workspace_configuration(valid_root.path()).expect("configuration");

	let malformed_root = tempfile::tempdir().expect("tempdir");
	fs::write(
		malformed_root.path().join("monochange.toml"),
		"not valid toml [[[",
	)
	.expect("write malformed config");

	let error = try_run_publish_packages_with_publications_and_resume(
		malformed_root.path(),
		&configuration,
		&[],
		&BTreeSet::new(),
		PublishPackagesOptions::default(),
	)
	.await
	.expect_err("malformed config should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Release);
	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_publish_packages_with_publications_maps_build_request_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration");
	let publication = PackagePublicationTarget {
		package: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		registry: Some(PublishRegistry::Custom("internal".to_string())),
		version: "1.0.0".to_string(),
		mode: PublishMode::Builtin,
		flow: monochange_core::PublishFlow::default(),
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
	};

	let error = try_run_publish_packages_with_publications_and_resume(
		root.path(),
		&configuration,
		&[publication],
		&BTreeSet::new(),
		PublishPackagesOptions::default(),
	)
	.await
	.expect_err("custom registry should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Release);
	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_publish_packages_with_publications_maps_resume_artifact_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration");
	let missing_resume = root.path().join("missing-resume.json");

	let error = try_run_publish_packages_with_publications_and_resume(
		root.path(),
		&configuration,
		&[],
		&BTreeSet::new(),
		PublishPackagesOptions {
			resume_path: Some(&missing_resume),
			..PublishPackagesOptions::default()
		},
	)
	.await
	.expect_err("missing resume artifact should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Release);
	assert!(report.packages.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_run_publish_packages_with_publications_maps_invalid_resume_report_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration");
	let resume_path = root.path().join("resume.json");
	fs::write(
		&resume_path,
		r#"{ "mode": "release", "dry_run": true, "packages": [] }"#,
	)
	.expect("write invalid resume report");

	let error = try_run_publish_packages_with_publications_and_resume(
		root.path(),
		&configuration,
		&[],
		&BTreeSet::new(),
		PublishPackagesOptions {
			resume_path: Some(&resume_path),
			..PublishPackagesOptions::default()
		},
	)
	.await
	.expect_err("invalid resume report should produce a failure");
	let report = error.into_report();
	assert_eq!(report.mode, PackagePublishRunMode::Release);
	assert!(report.packages.is_empty());
}
