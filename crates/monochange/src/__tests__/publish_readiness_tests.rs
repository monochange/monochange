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
					// macOS accepted sockets inherit the listener's O_NONBLOCK
					// flag; switch back to blocking before reading or writing.
					stream
						.set_nonblocking(false)
						.unwrap_or_else(|error| panic!("set blocking: {error}"));
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
		rate_limit: None,
		deferred_to: None,
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
		swift: monochange_core::EcosystemSettings::default(),
		cocoapods: monochange_core::EcosystemSettings::default(),
		gradle: monochange_core::EcosystemSettings::default(),
		version_schemes: BTreeMap::new(),
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
		version_source: monochange_core::VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
		publish: monochange_core::PublishSettings::default(),
		cli: None,
		display_version: None,
		values: BTreeMap::new(),
	}
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
	insta::assert_snapshot!(text);
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
	assert!(
		blocked_text.contains("Trusted publishing (1)\n  core  blocked  blocked"),
		"{blocked_text}"
	);

	let disabled_report = sample_readiness_report(vec![PublishReadinessPackage {
		trusted_publishing: Some(TrustedPublishingReadiness {
			status: TrustedPublishingReadinessStatus::Disabled,
			message: "trusted publishing is disabled".to_string(),
		}),
		..sample_readiness_package()
	}]);
	let disabled_text = render_report(&disabled_report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("disabled text report: {error}"));
	assert!(
		disabled_text.contains("  core  disabled  trusted publishing is disabled"),
		"{disabled_text}"
	);
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
		flow: monochange_core::PublishFlow::Direct,
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
		release_tag: None,
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
			flow: monochange_core::PublishFlow::Direct,
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
	assert!(package.message.contains("monochange publish placeholder"));
	let trust = package.trusted_publishing.as_ref().unwrap();
	assert_eq!(
		trust.status,
		crate::trusted_publishing_readiness::TrustedPublishingReadinessStatus::Blocked
	);
}

#[test]
fn blocked_readiness_reports_end_with_the_blocked_count() {
	let mut blocked = sample_readiness_package();
	blocked.status = PublishReadinessPackageStatus::Blocked;
	blocked.message = "version already exists with different content".to_string();
	let mut published = sample_readiness_package();
	published.package = "app".to_string();
	published.status = PublishReadinessPackageStatus::AlreadyPublished;
	let mut unsupported = sample_readiness_package();
	unsupported.package = "docs".to_string();

	unsupported.status = PublishReadinessPackageStatus::Unsupported;
	let mut report = sample_readiness_report(vec![blocked, published, unsupported]);
	report.status = PublishReadinessGlobalStatus::Blocked;
	report.order_findings.push(PublishOrderFinding {
		package: Some("core".to_string()),
		message: "core must publish before app".to_string(),
		blocking: true,
	});

	let text = render_report(&report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("text report: {error}"));
	insta::assert_snapshot!(text);

	let empty = render_report(&sample_readiness_report(Vec::new()), OutputFormat::Text)
		.unwrap_or_else(|error| panic!("empty text report: {error}"));
	assert!(
		empty.contains("No packages are selected for publishing."),
		"{empty}"
	);
}
