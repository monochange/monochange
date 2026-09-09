#![allow(clippy::disallowed_methods)]
use super::*;

fn trusted_registry_client() -> monochange_publish::Client {
	monochange_publish::registry_client().unwrap_or_else(|error| panic!("registry client: {error}"))
}

fn trusted_registry_not_found() -> &'static [u8] {
	b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
}

fn trusted_registry_mock(
	response: &'static [u8],
) -> (
	monochange_publish::RegistryEndpoints,
	std::thread::JoinHandle<()>,
) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind registry mock: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("registry mock address: {error}"));
	listener
		.set_nonblocking(true)
		.unwrap_or_else(|error| panic!("set nonblocking: {error}"));
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
	let thread = std::thread::spawn(move || {
		while std::time::Instant::now() < deadline {
			match listener.accept() {
				Ok((mut stream, _)) => {
					let mut request = [0_u8; 2048];
					let _ = std::io::Read::read(&mut stream, &mut request);
					let _ = std::io::Write::write_all(&mut stream, response);
					return;
				}
				Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
					std::thread::sleep(std::time::Duration::from_millis(25));
				}
				Err(_) => return,
			}
		}
	});
	let mut endpoints = monochange_publish::RegistryEndpoints::from_env();
	endpoints.npm_registry = format!("http://{address}");
	(endpoints, thread)
}

async fn validate_publish_readiness_artifact(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	artifact_path: &Path,
) -> MonochangeResult<()> {
	let artifact = read_report_artifact(artifact_path)?;
	let current_report = build_publish_readiness_report_for_publish(
		root,
		configuration,
		prepared_release,
		selected_packages,
	)
	.await?;
	validate_publish_readiness_report(&artifact, &current_report)
}

fn validate_publish_readiness_report(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	validate_readiness_artifact_header(artifact)?;
	validate_readiness_artifact_status(artifact)?;
	validate_readiness_current_status(current)?;
	validate_readiness_release_commit(artifact, current)?;
	validate_readiness_input_fingerprint(artifact, current)?;
	validate_readiness_packages(artifact, current)
}

fn validate_readiness_artifact_status(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.status == PublishReadinessGlobalStatus::Ready {
		return Ok(());
	}
	Err(MonochangeError::Config(
		"publish readiness artifact is blocked; rerun `monochange step publish-readiness` and resolve blockers before `monochange step publish-packages`".to_string(),
	))
}

fn validate_readiness_current_status(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.status == PublishReadinessGlobalStatus::Ready {
		return Ok(());
	}
	Err(MonochangeError::Config(
		"current publish readiness is blocked; rerun `monochange step publish-readiness` and resolve blockers before `monochange step publish-packages`".to_string(),
	))
}

fn validate_readiness_packages(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	let artifact_packages = package_identities(&artifact.packages)?;
	let current_packages = package_identities(&current.packages)?;
	if artifact.package_set_fingerprint != package_set_fingerprint(&artifact.packages) {
		return Err(MonochangeError::Config(
			"publish readiness artifact package fingerprint does not match its package list"
				.to_string(),
		));
	}
	if artifact_packages == current_packages {
		return Ok(());
	}
	let missing = current_packages
		.difference(&artifact_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();
	let stale = artifact_packages
		.difference(&current_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();
	Err(MonochangeError::Config(format!(
		"publish readiness artifact package set is stale or does not match selected packages (missing: {}; stale: {})",
		render_package_identity_list(&missing),
		render_package_identity_list(&stale)
	)))
}

fn sample_publish_outcome(
	status: package_publish::PackagePublishStatus,
) -> package_publish::PackagePublishOutcome {
	package_publish::PackagePublishOutcome {
		package: "core".to_string(),
		ecosystem: Ecosystem::Cargo,
		registry: "crates_io".to_string(),
		version: "1.2.3".to_string(),
		status,
		message: "ready to publish core 1.2.3".to_string(),
		placeholder: false,
		trusted_publishing: package_publish::TrustedPublishingOutcome {
			status: package_publish::TrustedPublishingStatus::Disabled,
			repository: None,
			workflow: None,
			environment: None,
			setup_url: None,
			message: "trusted publishing disabled".to_string(),
		},
		command: None,
		stdout: None,
		stderr: None,
	}
}

fn sample_source() -> PublishReadinessSource<'static> {
	PublishReadinessSource {
		from: "HEAD",
		resolved_commit: "resolved123",
		record_commit: "record123",
	}
}

fn sample_readiness_report(packages: Vec<PublishReadinessPackage>) -> PublishReadinessReport {
	PublishReadinessReport {
		schema_version: PUBLISH_READINESS_SCHEMA_VERSION,
		kind: PUBLISH_READINESS_KIND.to_string(),
		status: PublishReadinessGlobalStatus::Ready,
		from: "HEAD".to_string(),
		resolved_commit: "resolved123".to_string(),
		record_commit: "record123".to_string(),
		package_set_fingerprint: package_set_fingerprint(&packages),
		input_fingerprint: "fnv1a64:sample".to_string(),
		packages,
		publish_order: Vec::new(),
		order_findings: Vec::new(),
	}
}

fn sample_report_context(root: &Path) -> ReportBuildContext<'_> {
	ReportBuildContext {
		root,
		configuration: Box::leak(Box::new(sample_configuration(root))),
		source: None,
		record_order: None,
		requests: Vec::new(),
		workspace_packages: Vec::new(),
		env_map: BTreeMap::new(),
		registry_transport: None,
	}
}

fn sample_readiness_package() -> PublishReadinessPackage {
	PublishReadinessPackage {
		package: "core".to_string(),
		ecosystem: Ecosystem::Cargo,
		registry: "crates.io".to_string(),
		version: "1.2.3".to_string(),
		status: PublishReadinessPackageStatus::Ready,
		message: "ready to publish core 1.2.3".to_string(),
		trusted_publishing: None,
	}
}

fn readiness_package(
	package: &str,
	registry: &str,
	status: PublishReadinessPackageStatus,
) -> PublishReadinessPackage {
	PublishReadinessPackage {
		package: package.to_string(),
		registry: registry.to_string(),
		status,
		message: format!("{package} readiness"),
		..sample_readiness_package()
	}
}

fn sample_configuration(root: &Path) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: Vec::new(),
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
	}
}

fn sample_package_definition(
	id: &str,
	path: &str,
	package_type: PackageType,
) -> monochange_core::PackageDefinition {
	monochange_core::PackageDefinition {
		id: id.to_string(),
		path: PathBuf::from(path),
		package_type,
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
		version_format: monochange_core::VersionFormat::default(),
		publish: monochange_core::PublishSettings::default(),
	}
}

fn sample_prepared_release(root: &Path) -> PreparedRelease {
	PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: root.to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: Vec::new(),
		package_publications: Vec::new(),
		version: None,
		group_version: None,
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
	}
}

#[tokio::test]
async fn build_report_maps_publish_dry_run_statuses_to_readiness_statuses() {
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![
			sample_publish_outcome(package_publish::PackagePublishStatus::Planned),
			sample_publish_outcome(package_publish::PackagePublishStatus::SkippedExisting),
			sample_publish_outcome(package_publish::PackagePublishStatus::SkippedExternal),
			sample_publish_outcome(package_publish::PackagePublishStatus::Blocked),
		],
	};
	let readiness = build_report_from_publish_report(
		sample_report_context(Path::new(".")),
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	assert_eq!(readiness.schema_version, PUBLISH_READINESS_SCHEMA_VERSION);
	assert_eq!(readiness.kind, PUBLISH_READINESS_KIND);
	assert_eq!(readiness.from, "HEAD");
	assert_eq!(readiness.resolved_commit, "resolved123");
	assert_eq!(readiness.record_commit, "record123");
	assert_eq!(readiness.input_fingerprint, "fnv1a64:sample");
	assert_eq!(readiness.status, PublishReadinessGlobalStatus::Blocked);
	assert_eq!(
		readiness.packages[0].status,
		PublishReadinessPackageStatus::Ready
	);
	assert_eq!(
		readiness.packages[1].status,
		PublishReadinessPackageStatus::AlreadyPublished
	);
	assert_eq!(
		readiness.packages[2].status,
		PublishReadinessPackageStatus::Unsupported
	);
	assert_eq!(
		readiness.packages[3].status,
		PublishReadinessPackageStatus::Blocked
	);

	assert!(!readiness.package_set_fingerprint.is_empty());
}

#[test]
fn publish_readiness_input_fingerprint_tracks_publish_inputs() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut configuration = sample_configuration(root);
	configuration.packages = vec![
		sample_package_definition("cargo", "crates/core", PackageType::Cargo),
		sample_package_definition("npm", "packages/web", PackageType::Npm),
		sample_package_definition("deno", "packages/deno", PackageType::Deno),
		sample_package_definition("dart", "packages/dart", PackageType::Dart),
		sample_package_definition("dart", "packages/flutter", PackageType::Dart),
		sample_package_definition("python", "packages/python", PackageType::Python),
	];

	write_test_file(root.join("monochange.toml"), b"[workspace]\n");
	write_test_file(root.join("Cargo.toml"), b"[workspace]\n");
	write_test_file(root.join("package.json"), br#"{"private":true}"#);
	write_test_file(root.join("pnpm-lock.yaml"), b"lockfileVersion: '9.0'\n");
	write_test_file(root.join(".npmrc"), b"provenance=true\n");
	write_test_file(
		root.join("crates/core/Cargo.toml"),
		b"[package]\nname='core'\n",
	);
	write_test_file(root.join("crates/core/Cargo.lock"), b"version = 4\n");
	write_test_file(root.join("packages/web/package.json"), br#"{"name":"web"}"#);
	write_test_file(
		root.join("packages/web/pnpm-lock.yaml"),
		b"lockfileVersion: '9.0'\n",
	);
	write_test_file(root.join("packages/deno/deno.jsonc"), b"{}\n");
	write_test_file(root.join("packages/dart/pubspec.yaml"), b"name: dart\n");
	write_test_file(
		root.join("packages/flutter/pubspec.yaml"),
		b"name: flutter\n",
	);
	write_test_file(
		root.join("packages/python/pyproject.toml"),
		b"[project]\nname='python'\n",
	);

	let paths = publish_readiness_input_paths(root, &configuration);
	let relative_paths: BTreeSet<_> = paths
		.iter()
		.map(|path| readiness_relative_path(root, path))
		.collect();
	let expected_paths = BTreeSet::from([
		".npmrc".to_string(),
		"Cargo.toml".to_string(),
		"crates/core/Cargo.lock".to_string(),
		"crates/core/Cargo.toml".to_string(),
		"monochange.toml".to_string(),
		"package.json".to_string(),
		"packages/dart/pubspec.yaml".to_string(),
		"packages/deno/deno.jsonc".to_string(),
		"packages/flutter/pubspec.yaml".to_string(),
		"packages/python/pyproject.toml".to_string(),
		"packages/web/package.json".to_string(),
		"packages/web/pnpm-lock.yaml".to_string(),
		"pnpm-lock.yaml".to_string(),
	]);
	assert_eq!(relative_paths, expected_paths);
	assert!(package_manifest_names_for_type("unknown").is_empty());

	let initial_fingerprint = publish_readiness_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("initial fingerprint: {error}"));
	write_test_file(
		root.join("packages/web/package.json"),
		br#"{"name":"web","type":"module"}"#,
	);
	let changed_fingerprint = publish_readiness_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("changed fingerprint: {error}"));

	assert_ne!(initial_fingerprint, changed_fingerprint);
	assert!(initial_fingerprint.starts_with("fnv1a64:"));
}

#[cfg(unix)]
#[test]
fn publish_readiness_input_fingerprint_reports_read_errors() {
	use std::os::unix::fs::PermissionsExt;

	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let configuration = sample_configuration(root);
	let input_path = root.join("monochange.toml");
	write_test_file(&input_path, b"[workspace]\n");
	fs::set_permissions(&input_path, fs::Permissions::from_mode(0o000))
		.unwrap_or_else(|error| panic!("remove read permission: {error}"));

	let error = publish_readiness_input_fingerprint(root, &configuration)
		.expect_err("unreadable input should report a read error");

	fs::set_permissions(&input_path, fs::Permissions::from_mode(0o600))
		.unwrap_or_else(|error| panic!("restore read permission: {error}"));
	assert!(
		error
			.to_string()
			.contains("failed to read publish readiness input")
	);
}

#[test]
fn validate_publish_readiness_report_rejects_stale_input_fingerprints() {
	let mut artifact = sample_readiness_report(vec![sample_readiness_package()]);
	let current = artifact.clone();
	artifact.input_fingerprint = "fnv1a64:stale".to_string();

	let error = validate_publish_readiness_report(&artifact, &current)
		.expect_err("stale input fingerprint should be rejected");

	assert!(error.to_string().contains("inputs are stale"));
}

fn write_test_file(path: impl AsRef<Path>, contents: &[u8]) {
	let path = path.as_ref();
	let parent = path.parent().unwrap_or(Path::new("."));
	fs::create_dir_all(parent)
		.unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
	fs::write(path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

#[test]
fn render_report_supports_json_text_and_markdown() {
	let mut report = sample_readiness_report(vec![sample_readiness_package()]);
	report.publish_order = vec!["core".to_string()];
	report.order_findings.push(PublishOrderFinding {
		package: None,
		message: "release record publication order differs".to_string(),
		blocking: false,
	});
	report.packages[0].trusted_publishing = Some(TrustedPublishingReadiness {
		status: TrustedPublishingReadinessStatus::ManualVerificationRequired,
		message: "verify the registry-side trusted publisher configuration".to_string(),
	});

	let text = render_report(&report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("text report: {error}"));
	assert!(text.contains("publish readiness: ready"));
	assert!(text.contains("release record: record123"));
	assert!(text.contains("trusted publishing [manual_verification_required]"));
	assert!(text.contains("publish order: core"));
	assert!(text.contains("order finding [note]: release record publication order differs"));
	let markdown = render_report(&report, OutputFormat::Markdown)
		.unwrap_or_else(|error| panic!("markdown report: {error}"));
	assert!(markdown.contains("## Publish readiness"));
	assert!(markdown.contains("Release record: `record123`"));
	assert!(markdown.contains("Trusted publishing"));
	assert!(markdown.contains("Publish order: `core`"));
	assert!(markdown.contains("Order finding (note)"));
	let json = render_report(&report, OutputFormat::Json)
		.unwrap_or_else(|error| panic!("json report: {error}"));
	assert!(json.contains("\"status\": \"ready\""));
	assert!(json.contains("\"kind\": \"monochange.publishReadiness\""));
	assert!(json.contains("\"trusted_publishing\""));
	assert!(json.contains("\"publish_order\""));
	assert!(json.contains("\"order_findings\""));

	let blocked_report = sample_readiness_report(vec![PublishReadinessPackage {
		trusted_publishing: Some(TrustedPublishingReadiness {
			status: TrustedPublishingReadinessStatus::Blocked,
			message: "blocked".to_string(),
		}),
		..sample_readiness_package()
	}]);
	let blocked_text = render_report(&blocked_report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("blocked text report: {error}"));
	assert!(blocked_text.contains("trusted publishing [blocked]"));

	let disabled_report = sample_readiness_report(vec![PublishReadinessPackage {
		trusted_publishing: Some(TrustedPublishingReadiness {
			status: TrustedPublishingReadinessStatus::Disabled,
			message: "trusted publishing is disabled".to_string(),
		}),
		..sample_readiness_package()
	}]);
	let disabled_text = render_report(&disabled_report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("disabled text report: {error}"));
	assert!(disabled_text.contains("trusted publishing [disabled]"));
}

#[test]
fn deserialize_report_uses_schema_and_kind_defaults() {
	let report: PublishReadinessReport = serde_json::from_value(serde_json::json!({
		"status": "ready",
		"from": "HEAD",
		"resolved_commit": "resolved123",
		"record_commit": "record123",
		"package_set_fingerprint": "packages:none",
		"packages": []
	}))
	.unwrap_or_else(|error| panic!("deserialize defaulted report: {error}"));

	assert_eq!(report.schema_version, PUBLISH_READINESS_SCHEMA_VERSION);
	assert_eq!(report.kind, PUBLISH_READINESS_KIND);
}

#[test]
fn render_report_handles_empty_package_sections() {
	let report = sample_readiness_report(Vec::new());
	let text = render_report(&report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("empty text report: {error}"));
	let markdown = render_report(&report, OutputFormat::Markdown)
		.unwrap_or_else(|error| panic!("empty markdown report: {error}"));

	assert!(text.contains("packages: none"));
	assert!(markdown.contains("No packages selected for publishing."));
}

#[test]
fn publish_readiness_json_error_renders_context() {
	let error = serde_json::from_str::<serde_json::Value>("{")
		.expect_err("invalid JSON should produce serde error");
	let error = publish_readiness_json_error(error);

	assert!(error.to_string().contains("publish readiness JSON"));
}

#[test]
fn render_report_labels_blocked_already_published_and_unsupported_packages() {
	let packages = vec![
		PublishReadinessPackage {
			status: PublishReadinessPackageStatus::AlreadyPublished,
			..sample_readiness_package()
		},
		PublishReadinessPackage {
			package: "external".to_string(),
			status: PublishReadinessPackageStatus::Unsupported,
			..sample_readiness_package()
		},
		PublishReadinessPackage {
			package: "blocked".to_string(),
			status: PublishReadinessPackageStatus::Blocked,
			..sample_readiness_package()
		},
	];
	let report = PublishReadinessReport {
		status: PublishReadinessGlobalStatus::Blocked,
		package_set_fingerprint: package_set_fingerprint(&packages),
		packages,
		..sample_readiness_report(Vec::new())
	};
	let markdown = render_report(&report, OutputFormat::Markdown)
		.unwrap_or_else(|error| panic!("blocked markdown report: {error}"));

	assert!(markdown.contains("Status: `blocked`"));
	assert!(markdown.contains("already_published"));
	assert!(markdown.contains("unsupported"));
	assert!(markdown.contains("blocked"));
}

#[test]
fn write_and_read_report_artifact_cover_success_and_io_errors() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let report = sample_readiness_report(vec![sample_readiness_package()]);
	let output = tempdir.path().join("nested/readiness.json");

	write_report_artifact(&output, &report)
		.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
	let body = fs::read_to_string(&output)
		.unwrap_or_else(|error| panic!("read readiness artifact: {error}"));
	assert!(body.contains("\"package\": \"core\""));
	let loaded = read_report_artifact(&output)
		.unwrap_or_else(|error| panic!("load readiness artifact: {error}"));
	assert_eq!(loaded, report);

	let missing_error = read_report_artifact(&tempdir.path().join("missing.json"))
		.expect_err("missing readiness artifact should fail");
	assert!(
		missing_error
			.to_string()
			.contains("failed to read publish readiness artifact")
	);
	fs::write(&output, "{").unwrap_or_else(|error| panic!("write invalid json: {error}"));
	let parse_error =
		read_report_artifact(&output).expect_err("invalid readiness artifact should fail");
	assert!(parse_error.to_string().contains("publish readiness JSON"));

	let parent_file = tempdir.path().join("parent-file");
	fs::write(&parent_file, "not a directory")
		.unwrap_or_else(|error| panic!("write parent file: {error}"));
	let create_dir_error = write_report_artifact(&parent_file.join("readiness.json"), &report)
		.expect_err("file parent should fail directory creation");
	assert!(
		create_dir_error
			.to_string()
			.contains("failed to create publish readiness output directory")
	);

	let write_error = write_report_artifact(tempdir.path(), &report)
		.expect_err("directory output should fail file write");
	assert!(
		write_error
			.to_string()
			.contains("failed to write publish readiness output")
	);
}

#[test]
fn validate_publish_readiness_report_accepts_matching_ready_reports() {
	let artifact = sample_readiness_report(vec![sample_readiness_package()]);
	let mut current = artifact.clone();
	current.from = "HEAD".to_string();

	validate_publish_readiness_report(&artifact, &current)
		.unwrap_or_else(|error| panic!("matching readiness artifact: {error}"));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_publish_readiness_artifact_accepts_prepared_release_reports() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let artifact_path = root.join("readiness.json");
	let configuration = sample_configuration(root);
	let prepared_release = sample_prepared_release(root);
	let selected_packages = BTreeSet::new();
	let report = build_publish_readiness_report_for_publish(
		root,
		&configuration,
		Some(&prepared_release),
		&selected_packages,
	)
	.await
	.unwrap_or_else(|error| panic!("prepared release readiness: {error}"));

	assert_eq!(report.from, "prepared-release");
	write_report_artifact(&artifact_path, &report)
		.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
	validate_publish_readiness_artifact(
		root,
		&configuration,
		Some(&prepared_release),
		&selected_packages,
		&artifact_path,
	)
	.await
	.unwrap_or_else(|error| panic!("validate readiness artifact: {error}"));
}

#[test]
fn publish_plan_ready_package_ids_requires_every_package_row_to_be_ready() {
	let report = sample_readiness_report(vec![
		readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package(
			"core",
			"npm",
			PublishReadinessPackageStatus::AlreadyPublished,
		),
		readiness_package("web", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package("web", "npm", PublishReadinessPackageStatus::Blocked),
		readiness_package(
			"docs",
			"crates.io",
			PublishReadinessPackageStatus::AlreadyPublished,
		),
		readiness_package(
			"external",
			"crates.io",
			PublishReadinessPackageStatus::Unsupported,
		),
	]);

	let ready_packages = publish_plan_ready_package_ids(&report);

	assert_eq!(
		ready_packages,
		BTreeSet::from(["core".to_string(), "docs".to_string()])
	);
}

#[test]
fn validate_publish_readiness_plan_artifact_accepts_blocked_subset_reports() {
	let artifact = sample_readiness_report(vec![
		readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package("web", "crates.io", PublishReadinessPackageStatus::Blocked),
		readiness_package("extra", "crates.io", PublishReadinessPackageStatus::Ready),
	]);
	let current = PublishReadinessReport {
		status: PublishReadinessGlobalStatus::Blocked,
		packages: vec![
			readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
			readiness_package("web", "crates.io", PublishReadinessPackageStatus::Blocked),
		],
		..sample_readiness_report(Vec::new())
	};

	validate_publish_readiness_plan_artifact(&artifact, &current)
		.unwrap_or_else(|error| panic!("planning readiness artifact: {error}"));
}

#[test]
fn validate_publish_readiness_plan_artifact_rejects_tampering_and_missing_coverage() {
	let current = sample_readiness_report(vec![
		readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package("web", "crates.io", PublishReadinessPackageStatus::Ready),
	]);

	let mut tampered = current.clone();
	tampered.package_set_fingerprint = "tampered".to_string();
	let tampered_error = validate_publish_readiness_plan_artifact(&tampered, &current)
		.expect_err("tampered planning artifact should fail validation");
	assert!(tampered_error.to_string().contains("package fingerprint"));

	let missing = sample_readiness_report(vec![readiness_package(
		"core",
		"crates.io",
		PublishReadinessPackageStatus::Ready,
	)]);
	let missing_error = validate_publish_readiness_plan_artifact(&missing, &current)
		.expect_err("planning artifact missing current package should fail");
	assert!(
		missing_error
			.to_string()
			.contains("does not cover selected packages")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_plan_package_filter_from_readiness_artifact_accepts_empty_prepared_release() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let artifact_path = root.join("readiness.json");
	let configuration = sample_configuration(root);
	let prepared_release = sample_prepared_release(root);
	let selected_packages = BTreeSet::new();
	let report = build_publish_readiness_report_for_publish(
		root,
		&configuration,
		Some(&prepared_release),
		&selected_packages,
	)
	.await
	.unwrap_or_else(|error| panic!("prepared release readiness: {error}"));

	write_report_artifact(&artifact_path, &report)
		.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
	let planned_packages = publish_plan_package_filter_from_readiness_artifact(
		root,
		&configuration,
		Some(&prepared_release),
		&selected_packages,
		&artifact_path,
	)
	.await
	.unwrap_or_else(|error| panic!("publish plan readiness filter: {error}"));

	assert!(planned_packages.is_empty());
}

#[test]
fn validate_publish_readiness_report_rejects_bad_kind_schema_and_statuses() {
	let current = sample_readiness_report(vec![sample_readiness_package()]);

	let mut bad_kind = current.clone();
	bad_kind.kind = "other".to_string();
	let bad_kind_error = validate_publish_readiness_report(&bad_kind, &current)
		.expect_err("bad kind should fail readiness validation");
	assert!(bad_kind_error.to_string().contains("expected"));

	let mut bad_schema = current.clone();
	bad_schema.schema_version = 99;
	let bad_schema_error = validate_publish_readiness_report(&bad_schema, &current)
		.expect_err("bad schema should fail readiness validation");
	assert!(bad_schema_error.to_string().contains("not supported"));

	let mut blocked_artifact = current.clone();
	blocked_artifact.status = PublishReadinessGlobalStatus::Blocked;
	let blocked_artifact_error = validate_publish_readiness_report(&blocked_artifact, &current)
		.expect_err("blocked artifact should fail readiness validation");
	assert!(
		blocked_artifact_error
			.to_string()
			.contains("artifact is blocked")
	);

	let mut blocked_current = current.clone();
	blocked_current.status = PublishReadinessGlobalStatus::Blocked;
	let blocked_current_error = validate_publish_readiness_report(&current, &blocked_current)
		.expect_err("blocked current readiness should fail validation");
	assert!(
		blocked_current_error
			.to_string()
			.contains("current publish readiness is blocked")
	);
}

#[test]
fn validate_publish_readiness_report_rejects_stale_commits_and_packages() {
	let current = sample_readiness_report(vec![sample_readiness_package()]);

	let mut stale_commit = current.clone();
	stale_commit.record_commit = "old-record".to_string();
	let stale_commit_error = validate_publish_readiness_report(&stale_commit, &current)
		.expect_err("stale release record should fail validation");
	assert!(stale_commit_error.to_string().contains("old-record"));

	let mut missing_package = current.clone();
	missing_package.packages.clear();
	missing_package.package_set_fingerprint = package_set_fingerprint(&missing_package.packages);
	let missing_package_error = validate_publish_readiness_report(&missing_package, &current)
		.expect_err("missing package should fail validation");
	assert!(missing_package_error.to_string().contains("missing: core"));

	let mut stale_package = current.clone();
	stale_package.packages.push(PublishReadinessPackage {
		package: "web".to_string(),
		..sample_readiness_package()
	});
	stale_package.package_set_fingerprint = package_set_fingerprint(&stale_package.packages);
	let stale_package_error = validate_publish_readiness_report(&stale_package, &current)
		.expect_err("stale package should fail validation");
	assert!(stale_package_error.to_string().contains("stale: web"));
}

#[test]
fn validate_publish_readiness_report_rejects_tampered_fingerprint_and_duplicates() {
	let current = sample_readiness_report(vec![sample_readiness_package()]);

	let mut bad_fingerprint = current.clone();
	bad_fingerprint.package_set_fingerprint = "tampered".to_string();
	let bad_fingerprint_error = validate_publish_readiness_report(&bad_fingerprint, &current)
		.expect_err("tampered package fingerprint should fail validation");
	assert!(
		bad_fingerprint_error
			.to_string()
			.contains("package fingerprint")
	);

	let duplicate_package = PublishReadinessPackage {
		message: "duplicate".to_string(),
		..sample_readiness_package()
	};
	let mut duplicates =
		sample_readiness_report(vec![sample_readiness_package(), duplicate_package]);
	duplicates.package_set_fingerprint = package_set_fingerprint(&duplicates.packages);
	let duplicate_error = validate_publish_readiness_report(&duplicates, &current)
		.expect_err("duplicate package should fail validation");
	assert!(
		duplicate_error
			.to_string()
			.contains("duplicate package entry")
	);
}

#[test]
fn render_package_identity_list_labels_empty_lists() {
	assert_eq!(render_package_identity_list(&[]), "none");
	assert_eq!(
		render_package_identity_list(&["core Cargo crates.io 1.2.3".to_string()]),
		"core Cargo crates.io 1.2.3"
	);
	assert_eq!(
		render_package_identity_list(&[
			"core Cargo crates.io 1.2.3".to_string(),
			"web Npm npmjs 4.5.6".to_string(),
		]),
		"core Cargo crates.io 1.2.3, web Npm npmjs 4.5.6"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn build_publish_readiness_for_publish_falls_back_to_head_without_prepared_release() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::process::Command::new("git")
		.current_dir(root)
		.args(["init"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["config", "user.email", "monochange@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["config", "user.name", "monochange Tests"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["config", "commit.gpgsign", "false"])
		.output()
		.unwrap_or_else(|error| panic!("git config gpgsign: {error}"));
	fs::write(root.join("README.md"), "readme\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["add", "."])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["commit", "-m", "initial"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));

	let error = build_publish_readiness_report_for_publish(
		root,
		&sample_configuration(root),
		None,
		&BTreeSet::new(),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected missing release record error"));

	assert!(error.to_string().contains("no monochange release record"));
}

fn trust_request_for(
	registry: &str,
	version: &str,
	enabled: bool,
) -> monochange_publish::PublishRequest {
	let mut request = monochange_publish::PublishRequest {
		package_id: "core".to_string(),
		package_name: "core".to_string(),
		ecosystem: Ecosystem::Cargo,
		manifest_path: PathBuf::from("Cargo.toml"),
		package_root: PathBuf::from("."),
		registry: monochange_core::RegistryKind::CratesIo,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: monochange_core::PublishMode::Builtin,
		version: version.to_string(),
		placeholder: false,
		trusted_publishing: monochange_core::TrustedPublishingSettings {
			enabled,
			..Default::default()
		},
		attestations: monochange_core::PublishAttestationSettings::default(),
		timeout: monochange_core::PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: "placeholder".to_string(),
	};
	if registry != "crates_io" {
		request.package_id = "web".to_string();
		request.package_name = "web".to_string();
		request.ecosystem = Ecosystem::Npm;
		request.registry = monochange_core::RegistryKind::Npm;
	}
	request
}

#[tokio::test(flavor = "multi_thread")]
async fn build_report_attaches_trusted_publishing_findings_and_publish_order() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut context = sample_report_context(root);
	context.requests = vec![
		trust_request_for("crates_io", "1.2.3", false),
		trust_request_for("npm", "4.5.6", false),
	];
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![
			sample_publish_outcome(package_publish::PackagePublishStatus::Planned),
			{
				let mut outcome =
					sample_publish_outcome(package_publish::PackagePublishStatus::Planned);
				outcome.package = "web".to_string();
				outcome.ecosystem = Ecosystem::Npm;
				outcome.registry = "npmjs".to_string();
				outcome.version = "4.5.6".to_string();
				outcome
			},
		],
	};

	let readiness = build_report_from_publish_report(
		context,
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	assert_eq!(
		readiness.publish_order,
		vec!["core".to_string(), "web".to_string()]
	);
	assert!(readiness.order_findings.is_empty());
	let trust_findings: Vec<_> = readiness
		.packages
		.iter()
		.map(|package| package.trusted_publishing.as_ref().unwrap().status)
		.collect();
	assert_eq!(
		trust_findings,
		vec![
			crate::trusted_publishing_readiness::TrustedPublishingReadinessStatus::Disabled,
			crate::trusted_publishing_readiness::TrustedPublishingReadinessStatus::Disabled,
		]
	);
	assert_eq!(readiness.status, PublishReadinessGlobalStatus::Ready);
}

#[tokio::test(flavor = "multi_thread")]
async fn build_report_flags_packages_publishing_before_their_dependencies() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut configuration = sample_configuration(root);
	configuration.npm.publish_order.dependency_fields = Some(vec![
		"dependencies".to_string(),
		"devDependencies".to_string(),
	]);

	let mut app = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"app".to_string(),
		root.join("app/package.json"),
		root.to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	app.metadata
		.insert("config_id".to_string(), "app".to_string());
	app.declared_dependencies
		.push(monochange_core::PackageDependency {
			name: "ui".to_string(),
			kind: monochange_core::DependencyKind::Development,
			version_constraint: None,
			optional: false,
			source_field: Some("devDependencies".to_string()),
		});
	let mut ui = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"ui".to_string(),
		root.join("ui/package.json"),
		root.to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	ui.metadata
		.insert("config_id".to_string(), "ui".to_string());

	let npm_outcome = |package: &str, version: &str| {
		let mut outcome = sample_publish_outcome(package_publish::PackagePublishStatus::Planned);
		outcome.package = package.to_string();
		outcome.ecosystem = Ecosystem::Npm;
		outcome.registry = "npmjs".to_string();
		outcome.version = version.to_string();
		outcome
	};
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![npm_outcome("app", "2.0.0"), npm_outcome("ui", "1.0.0")],
	};

	let mut context = sample_report_context(root);
	context.configuration = Box::leak(Box::new(configuration));
	context.workspace_packages = vec![app, ui];

	let readiness = build_report_from_publish_report(
		context,
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	assert_eq!(
		readiness.publish_order,
		vec!["app".to_string(), "ui".to_string()]
	);
	let blocking: Vec<_> = readiness
		.order_findings
		.iter()
		.filter(|finding| finding.blocking)
		.collect();
	assert_eq!(blocking.len(), 1);
	assert_eq!(blocking[0].package.as_deref(), Some("app"));
	assert!(blocking[0].message.contains("ui"));
	assert!(blocking[0].message.contains("devDependencies"));

	let app_row = readiness
		.packages
		.iter()
		.find(|package| package.package == "app")
		.unwrap();
	assert_eq!(app_row.status, PublishReadinessPackageStatus::Blocked);
	assert_eq!(readiness.status, PublishReadinessGlobalStatus::Blocked);
}

#[tokio::test(flavor = "multi_thread")]
async fn build_report_notes_release_record_order_mismatch_without_blocking() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut app = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"app".to_string(),
		root.join("app/package.json"),
		root.to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	app.metadata
		.insert("config_id".to_string(), "app".to_string());
	let mut ui = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"ui".to_string(),
		root.join("ui/package.json"),
		root.to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	ui.metadata
		.insert("config_id".to_string(), "ui".to_string());

	let npm_outcome = |package: &str| {
		let mut outcome = sample_publish_outcome(package_publish::PackagePublishStatus::Planned);
		outcome.package = package.to_string();
		outcome.ecosystem = Ecosystem::Npm;
		outcome.registry = "npmjs".to_string();
		outcome
	};
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![npm_outcome("app"), npm_outcome("ui")],
	};

	let mut context = sample_report_context(root);
	context.workspace_packages = vec![app, ui];
	let record_publication = |package: &str| {
		PackagePublicationTarget {
			package: package.to_string(),
			ecosystem: Ecosystem::Npm,
			registry: None,
			version: "1.0.0".to_string(),
			mode: monochange_core::PublishMode::Builtin,
			trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
			attestations: monochange_core::PublishAttestationSettings::default(),
			timeout: monochange_core::PublishTimeoutSettings::default(),
			fail_on_duplicate: false,
		}
	};
	let record_order = vec![record_publication("ui"), record_publication("app")];
	context.record_order = Some(&record_order);

	let readiness = build_report_from_publish_report(
		context,
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	assert!(readiness.order_findings.iter().any(|finding| {
		!finding.blocking && finding.package.is_none() && finding.message.contains("release record")
	}));
	assert_eq!(readiness.status, PublishReadinessGlobalStatus::Ready);
}

#[tokio::test(flavor = "multi_thread")]
async fn build_report_falls_back_to_disabled_trust_when_requests_are_missing() {
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![sample_publish_outcome(
			package_publish::PackagePublishStatus::Planned,
		)],
	};

	let readiness = build_report_from_publish_report(
		sample_report_context(Path::new(".")),
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	let trust = readiness.packages[0].trusted_publishing.as_ref().unwrap();
	assert_eq!(trust.status, TrustedPublishingReadinessStatus::Disabled);
	assert!(trust.message.contains("could not be evaluated"));
}

#[tokio::test(flavor = "multi_thread")]
async fn build_report_handles_empty_publish_sets_without_order_findings() {
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: Vec::new(),
	};

	let readiness = build_report_from_publish_report(
		sample_report_context(Path::new(".")),
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	assert!(readiness.publish_order.is_empty());
	assert!(readiness.order_findings.is_empty());
	assert!(readiness.packages.is_empty());
	assert_eq!(readiness.status, PublishReadinessGlobalStatus::Ready);
}

#[tokio::test(flavor = "multi_thread")]
async fn build_report_blocks_packages_when_trusted_publishing_readiness_blocks() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut request = monochange_publish::PublishRequest {
		registry: monochange_core::RegistryKind::Npm,
		ecosystem: Ecosystem::Npm,
		..trust_request_for("crates_io", "1.2.3", true)
	};
	request.trusted_publishing.repository = Some("acme/widgets".to_string());
	request.trusted_publishing.workflow = Some("release.yml".to_string());
	std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
	std::fs::write(root.join(".github/workflows/release.yml"), "jobs: {}").unwrap();

	let mut context = sample_report_context(root);
	context.requests = vec![request];
	context.env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_REPOSITORY".to_string(), "acme/widgets".to_string()),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"acme/widgets/.github/workflows/release.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "publish".to_string()),
	]);
	let client = trusted_registry_client();
	let (endpoints, server) = trusted_registry_mock(trusted_registry_not_found());
	context.registry_transport = Some((&client, &endpoints));

	let mut outcome = sample_publish_outcome(package_publish::PackagePublishStatus::Planned);
	outcome.registry = "npm".to_string();
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: true,
		packages: vec![outcome],
	};

	let readiness = build_report_from_publish_report(
		context,
		sample_source(),
		&report,
		"fnv1a64:sample".to_string(),
	)
	.await
	.unwrap();

	server
		.join()
		.unwrap_or_else(|_| panic!("registry mock thread"));
	assert_eq!(readiness.status, PublishReadinessGlobalStatus::Blocked);
	let package = &readiness.packages[0];
	assert_eq!(package.status, PublishReadinessPackageStatus::Blocked);
	assert!(package.message.contains("placeholder-publish"));
	let trust = package.trusted_publishing.as_ref().unwrap();
	assert_eq!(
		trust.status,
		crate::trusted_publishing_readiness::TrustedPublishingReadinessStatus::Blocked
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn build_publish_readiness_report_handles_empty_release_publications() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let git = |args: &[&str]| {
		std::process::Command::new("git")
			.current_dir(root)
			.args(["-c", "commit.gpgsign=false"])
			.args(args)
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	};
	git(&["init"]);
	git(&["config", "user.name", "monochange-tests"]);
	git(&["config", "user.email", "monochange-tests@example.com"]);
	fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n",
	)
	.unwrap();
	let releases = root.join(".monochange/releases/abc123");
	fs::create_dir_all(&releases).unwrap();
	fs::write(
		releases.join("release.json"),
		r#"{
	"schema_version": "0.4",
	"kind": "monochange.releaseRecord",
	"created_at": "2026-04-07T00:00:00Z",
	"command": "release",
	"version": null,
	"versions": {},
	"release_targets": [],
	"released_packages": [],
	"changed_files": [],
	"package_publications": [],
	"updated_changelogs": [],
	"deleted_changesets": [],
	"changesets": [],
	"changelogs": [],
	"provider": null
}"#,
	)
	.unwrap();
	git(&["add", "."]);
	git(&["commit", "-m", "release"]);

	let report =
		build_publish_readiness_report(root, &sample_configuration(root), "HEAD", &BTreeSet::new())
			.await
			.unwrap_or_else(|error| panic!("build readiness report: {error}"));

	assert!(report.packages.is_empty());
	assert!(report.publish_order.is_empty());
	assert_eq!(report.status, PublishReadinessGlobalStatus::Ready);
}
