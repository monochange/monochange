#![allow(clippy::disallowed_methods)]
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use monochange_core::ChangelogSettings;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetRevision;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::PackageDefinition;
use monochange_core::PreparedChangeset;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ShellConfig;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use monochange_core::VersionSource;
use monochange_core::WorkspaceConfiguration;
use monochange_test_helpers::workspace_ops::collect_workspace_files;
use monochange_test_helpers::workspace_ops::copy_workspace_file;
use monochange_test_helpers::workspace_ops::copy_workspace_tree;
use monochange_test_helpers::workspace_ops::ensure_parent_directory;
use monochange_test_helpers::workspace_ops::read_optional_file;
use monochange_test_helpers::workspace_ops::remap_workspace_path;
use monochange_test_helpers::workspace_ops::run_lockfile_command;
use monochange_test_helpers::workspace_ops::strip_workspace_prefix;

use super::*;

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[cfg(unix)]
fn make_executable(path: &Path) {
	let metadata = fs::metadata(path).unwrap_or_else(|error| panic!("metadata: {error}"));
	let mut permissions = metadata.permissions();
	permissions.set_mode(0o755);
	fs::set_permissions(path, permissions)
		.unwrap_or_else(|error| panic!("set permissions: {error}"));
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

/// Runs a freshly written script again while `exec` reports `ETXTBSY`: a child
/// that another test forks while the script is still open for writing keeps
/// the write handle until it execs, and the kernel refuses to run the file.
fn retry_while_text_file_busy<T>(
	mut run: impl FnMut() -> MonochangeResult<T>,
) -> MonochangeResult<T> {
	let mut attempts = 0;

	loop {
		let result = run();

		match &result {
			Err(error) if attempts < 50 && error.to_string().contains("Text file busy") => {
				attempts += 1;
				std::thread::sleep(std::time::Duration::from_millis(20));
			}
			_ => return result,
		}
	}
}

fn sample_changeset_with_context() -> PreparedChangeset {
	PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("feature".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GenericGit,
			host: None,
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					id: None,
					login: Some("ifiokjr".to_string()),
					display_name: Some("Ifiok Jr.".to_string()),
					url: None,
					source: HostedActorSourceKind::CommitAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					sha: "abc1234567890".to_string(),
					short_sha: "abc1234".to_string(),
					url: None,
					authored_at: None,
					committed_at: None,
					author_name: Some("Ifiok Jr.".to_string()),
					author_email: Some("ifiok@example.com".to_string()),
				}),
				review_request: None,
			}),
			last_updated: None,
			related_issues: Vec::new(),
		}),
	}
}

fn sample_source(provider: SourceProvider) -> SourceConfiguration {
	SourceConfiguration {
		provider,

		host: match provider {
			SourceProvider::Gitea | SourceProvider::Forgejo => {
				Some("https://codeberg.org".to_string())
			}
			SourceProvider::GitHub | SourceProvider::GitLab => None,
		},

		api_url: None,

		owner: match provider {
			SourceProvider::GitLab => "group".to_string(),
			_ => "org".to_string(),
		},

		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn workspace_configuration_with_lockfile_commands() -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings {
			lockfile_commands: vec![LockfileCommandDefinition {
				command: "cargo metadata".to_string(),
				cwd: None,
				shell: ShellConfig::None,
			}],
			..monochange_core::EcosystemSettings::default()
		},
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
fn init_rendering_helpers_cover_duplicate_names_changelogs_and_package_types() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("crates/core/src")).unwrap();
	fs::create_dir_all(root.join("packages/core")).unwrap();
	fs::create_dir_all(root.join("apps/cli")).unwrap();
	fs::create_dir_all(root.join("apps/mobile")).unwrap();
	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/core\"]\n",
	)
	.unwrap();
	fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap();

	fs::write(
		root.join("packages/core/package.json"),
		r#"{ "name": "core", "version": "1.0.0" }"#,
	)
	.unwrap();
	fs::write(
		root.join("apps/cli/deno.json"),
		r#"{ "name": "@acme/cli", "version": "1.0.0" }"#,
	)
	.unwrap();
	fs::write(
		root.join("apps/mobile/pubspec.yaml"),
		"name: mobile\nversion: 1.0.0\nflutter:\n  uses-material-design: true\n",
	)
	.unwrap();
	fs::create_dir_all(root.join("packages/sdk")).unwrap();
	fs::write(
		root.join("packages/sdk/package.json"),
		r#"{ "name": "@acme/sdk", "version": "1.0.0" }"#,
	)
	.unwrap();
	fs::create_dir_all(root.join("packages/dart_pkg/lib")).unwrap();
	fs::write(
		root.join("packages/dart_pkg/pubspec.yaml"),
		"name: dart_pkg\nversion: 1.0.0\n",
	)
	.unwrap();
	fs::write(root.join("packages/core/changelog.md"), "# Changelog\n").unwrap();

	let rendered = render_annotated_init_config(root, None, None)
		.unwrap_or_else(|error| panic!("render annotated config: {error}"));
	assert!(rendered.contains("type = \"cargo\""));
	assert!(rendered.contains("type = \"npm\""));
	assert!(rendered.contains("type = \"deno\""));
	assert!(rendered.contains("type = \"dart\""));
	assert!(rendered.contains("type = \"dart\""));
	assert!(rendered.contains("changelog = \"packages/core/changelog.md\""));
	assert!(rendered.contains("packages/core"));

	assert!(
		rendered.contains("[package.\"@acme/sdk\"]"),
		"scoped npm ids must render as quoted TOML keys:\n{rendered}"
	);
	assert!(
		rendered.contains("[package.\"@acme/cli\"]"),
		"scoped deno ids must render as quoted TOML keys:\n{rendered}"
	);

	let parsed: toml::Value = toml::from_str(&rendered).unwrap_or_else(|error| {
		panic!("rendered config should be valid TOML: {error}\n{rendered}")
	});
	let package_table = parsed
		.get("package")
		.and_then(toml::Value::as_table)
		.unwrap_or_else(|| panic!("rendered config should declare a package table:\n{rendered}"));
	assert!(package_table.contains_key("@acme/sdk"));
	assert!(package_table.contains_key("@acme/cli"));

	assert_eq!(
		detect_default_changelog(root, &root.join("packages/core")),
		Some(PathBuf::from("packages/core/changelog.md"))
	);
	assert_eq!(
		package_type_for_ecosystem(Ecosystem::Cargo),
		PackageType::Cargo
	);
	assert_eq!(package_type_for_ecosystem(Ecosystem::Npm), PackageType::Npm);
	assert_eq!(
		package_type_for_ecosystem(Ecosystem::Deno),
		PackageType::Deno
	);
	assert_eq!(
		package_type_for_ecosystem(Ecosystem::Dart),
		PackageType::Dart
	);
}

#[test]
fn validate_and_discover_release_workspace_cover_fallback_and_errors() {
	let empty_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(empty_root.path().join("monochange.toml"), "").unwrap();
	validate_cargo_workspace_version_groups(empty_root.path())
		.unwrap_or_else(|error| panic!("validate empty workspace: {error}"));

	let fixture = setup_fixture("monochange/release-base");
	let configuration = load_workspace_configuration(fixture.path())
		.unwrap_or_else(|error| panic!("load config: {error}"));
	let report = discover_release_workspace(fixture.path(), &configuration)
		.unwrap_or_else(|error| panic!("discover release workspace: {error}"));
	assert_eq!(report.packages.len(), 2);

	let fallback = discover_release_workspace(
		fixture.path(),
		&WorkspaceConfiguration {
			packages: Vec::new(),
			..configuration.clone()
		},
	)
	.unwrap_or_else(|error| panic!("discover fallback workspace: {error}"));
	assert!(!fallback.packages.is_empty());

	let broken = WorkspaceConfiguration {
		packages: vec![PackageDefinition {
			id: "missing".to_string(),
			path: PathBuf::from("missing"),
			package_type: PackageType::Cargo,
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
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Primary,
			version_source: VersionSource::default(),
			initial_version: None,
			bump_ceiling: None,
			classification_enforced: None,

			floating_tags: Vec::new(),
			cli: None,
			display_version: None,
			values: BTreeMap::new(),
		}],
		..configuration
	};
	let error = discover_release_workspace(fixture.path(), &broken)
		.err()
		.unwrap_or_else(|| panic!("expected configured package discovery error"));
	assert!(
		error.to_string().contains("failed to read")
			|| error.to_string().contains("could not be discovered")
	);

	let undetected_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(undetected_root.path().join("empty-package"))
		.unwrap_or_else(|error| panic!("create empty package dir: {error}"));
	let undetected = WorkspaceConfiguration {
		root_path: undetected_root.path().to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults {
			package_type: Some(PackageType::Go),
			..monochange_core::WorkspaceDefaults::default()
		},
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: vec![PackageDefinition {
			id: "empty".to_string(),
			path: PathBuf::from("empty-package"),
			package_type: PackageType::Go,
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
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Primary,
			version_source: VersionSource::default(),
			initial_version: None,
			bump_ceiling: None,
			classification_enforced: None,

			floating_tags: Vec::new(),
			cli: None,
			display_version: None,
			values: BTreeMap::new(),
		}],
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
	let undetected_error = discover_release_workspace(undetected_root.path(), &undetected)
		.err()
		.unwrap_or_else(|| panic!("expected configured package none-discovered error"));
	assert!(
		undetected_error
			.to_string()
			.contains("configured package `empty` at empty-package could not be discovered")
	);

	let missing_manifest_root =
		tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(missing_manifest_root.path().join("packages/pkg"))
		.unwrap_or_else(|error| panic!("create package dir: {error}"));
	let missing_manifest = WorkspaceConfiguration {
		root_path: missing_manifest_root.path().to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults {
			package_type: Some(PackageType::Npm),
			..monochange_core::WorkspaceDefaults::default()
		},
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: vec![PackageDefinition {
			id: "pkg".to_string(),
			path: PathBuf::from("packages/pkg"),
			package_type: PackageType::Npm,
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
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Primary,
			version_source: VersionSource::default(),
			initial_version: None,
			bump_ceiling: None,
			classification_enforced: None,

			floating_tags: Vec::new(),
			cli: None,
			display_version: None,
			values: BTreeMap::new(),
		}],
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
	let missing_manifest_error =
		discover_release_workspace(missing_manifest_root.path(), &missing_manifest)
			.err()
			.unwrap_or_else(|| panic!("expected configured package discovery failure"));
	assert!(
		missing_manifest_error
			.to_string()
			.contains("could not be discovered")
			|| missing_manifest_error
				.to_string()
				.contains("failed to read")
	);

	let non_cargo_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(non_cargo_root.path().join("packages/web"))
		.unwrap_or_else(|error| panic!("create npm package dir: {error}"));
	fs::write(
		non_cargo_root.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "npm"

[package.web]
path = "packages/web"
"#,
	)
	.unwrap_or_else(|error| panic!("write npm-only monochange config: {error}"));
	fs::write(
		non_cargo_root.path().join("packages/web/package.json"),
		r#"{ "name": "web", "version": "1.0.0" }"#,
	)
	.unwrap_or_else(|error| panic!("write package.json: {error}"));
	validate_cargo_workspace_version_groups(non_cargo_root.path())
		.unwrap_or_else(|error| panic!("validate npm-only workspace: {error}"));
}

#[test]
fn snapshot_and_change_file_helpers_cover_error_paths() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let snapshot_error =
		snapshot_directory_files(root, &root.join("missing-dir"), &mut BTreeMap::new())
			.err()
			.unwrap_or_else(|| panic!("expected snapshot error"));
	assert!(snapshot_error.to_string().contains("failed to read"));

	let fixture = setup_fixture("monochange/release-base");
	let invalid_version_error = add_change_file(
		fixture.path(),
		AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("reason")
			.version(Some("not-a-version"))
			.build(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(
		invalid_version_error
			.to_string()
			.contains("invalid explicit version")
	);

	let blocking_parent = fixture.path().join("blocked");
	fs::write(&blocking_parent, "file").unwrap();
	let write_error = add_interactive_change_file(
		fixture.path(),
		&interactive::InteractiveChangeResult {
			targets: vec![interactive::InteractiveTarget {
				id: "core".to_string(),
				bump: BumpSeverity::Patch,
				version: None,
				change_type: None,
			}],
			caused_by: Vec::new(),
			reason: "reason".to_string(),
			details: None,
		},
		Some(&blocking_parent.join("feature.md")),
	)
	.err()
	.unwrap_or_else(|| panic!("expected interactive write error"));
	assert!(write_error.to_string().contains("failed to create"));

	let blocking_output = fixture.path().join(".changeset");
	let add_change_write_error = add_change_file(
		fixture.path(),
		AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("reason")
			.output(Some(blocking_output.as_path()))
			.build(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected add_change_file write error"));
	assert!(
		add_change_write_error
			.to_string()
			.contains("failed to write")
	);

	let blocked_parent = fixture.path().join("parent-file");
	fs::write(&blocked_parent, "file").unwrap();
	let add_change_parent_error = add_change_file(
		fixture.path(),
		AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("reason")
			.output(Some(blocked_parent.join("feature.md").as_path()))
			.build(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected add_change_file parent creation error"));
	assert!(
		add_change_parent_error
			.to_string()
			.contains("failed to create")
	);

	let interactive_write_error = add_interactive_change_file(
		fixture.path(),
		&interactive::InteractiveChangeResult {
			targets: vec![interactive::InteractiveTarget {
				id: "core".to_string(),
				bump: BumpSeverity::Patch,
				version: None,
				change_type: None,
			}],
			caused_by: Vec::new(),
			reason: "reason".to_string(),
			details: Some("extra details".to_string()),
		},
		Some(blocking_output.as_path()),
	)
	.err()
	.unwrap_or_else(|| panic!("expected add_interactive_change_file write error"));
	assert!(
		interactive_write_error
			.to_string()
			.contains("failed to write")
	);

	let interactive_default_output = add_interactive_change_file(
		fixture.path(),
		&interactive::InteractiveChangeResult {
			targets: vec![interactive::InteractiveTarget {
				id: "core".to_string(),
				bump: BumpSeverity::Patch,
				version: None,
				change_type: None,
			}],
			caused_by: Vec::new(),
			reason: "reason".to_string(),
			details: None,
		},
		None,
	)
	.unwrap_or_else(|error| panic!("write interactive default changeset: {error}"));
	assert!(interactive_default_output.starts_with(fixture.path().join(".changeset")));
	assert!(interactive_default_output.exists());
}

#[test]
fn run_lockfile_command_in_place_reports_failures() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let parse_error = run_lockfile_command_in_place(
		root,
		&LockfileCommandExecution {
			command: "'".to_string(),
			cwd: root.to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected parse error"));
	assert!(parse_error.to_string().contains("failed to parse command"));

	let empty_error = run_lockfile_command_in_place(
		root,
		&LockfileCommandExecution {
			command: "   ".to_string(),
			cwd: root.to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected empty command error"));
	assert!(
		empty_error
			.to_string()
			.contains("lockfile command must not be empty")
	);

	let spawn_error = run_lockfile_command_in_place(
		root,
		&LockfileCommandExecution {
			command: "definitely-not-a-real-command".to_string(),
			cwd: root.to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected spawn error"));
	assert!(
		spawn_error
			.to_string()
			.contains("failed to run lockfile command")
	);

	let error = run_lockfile_command_in_place(
		root,
		&LockfileCommandExecution {
			command: "false".to_string(),
			cwd: root.to_path_buf(),
			shell: ShellConfig::Default,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected error from failing command"));

	assert!(error.to_string().contains("failed"));

	let both_script = root.join("both.sh");
	fs::write(
		&both_script,
		"#!/bin/sh\necho useful stdout\necho bad stderr >&2\nexit 8\n",
	)
	.unwrap_or_else(|error| panic!("write both script: {error}"));
	fs::set_permissions(&both_script, fs::Permissions::from_mode(0o755))
		.unwrap_or_else(|error| panic!("chmod both script: {error}"));
	let both_error = retry_while_text_file_busy(|| {
		run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: both_script.display().to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::None,
			},
		)
	})
	.err()
	.unwrap_or_else(|| panic!("expected both-streams error"));
	let both_text = both_error.to_string();
	assert!(both_text.contains("useful stdout"), "{both_text}");
	assert!(both_text.contains("bad stderr"), "{both_text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_source_changeset_context_dispatches_non_dry_gitlab_and_gitea_enrichment() {
	let mut gitlab_changesets = vec![sample_changeset_with_context()];
	apply_source_changeset_context(
		&sample_source(SourceProvider::GitLab),
		false,
		&mut gitlab_changesets,
	)
	.await;
	let gitlab_context = gitlab_changesets
		.first()
		.and_then(|changeset| changeset.context.as_ref())
		.unwrap_or_else(|| panic!("expected GitLab context"));
	assert_eq!(gitlab_context.provider, HostingProviderKind::GitLab);
	assert_eq!(gitlab_context.host.as_deref(), Some("gitlab.com"));
	assert_eq!(
		gitlab_context
			.introduced
			.as_ref()
			.and_then(|revision| revision.commit.as_ref())
			.and_then(|commit| commit.url.as_deref()),
		Some("https://gitlab.com/group/monochange/-/commit/abc1234567890")
	);

	let mut gitea_changesets = vec![sample_changeset_with_context()];
	apply_source_changeset_context(
		&sample_source(SourceProvider::Gitea),
		false,
		&mut gitea_changesets,
	)
	.await;
	let gitea_context = gitea_changesets
		.first()
		.and_then(|changeset| changeset.context.as_ref())
		.unwrap_or_else(|| panic!("expected Gitea context"));
	assert_eq!(gitea_context.provider, HostingProviderKind::Gitea);
	assert_eq!(gitea_context.host.as_deref(), Some("codeberg.org"));
	assert_eq!(
		gitea_context
			.introduced
			.as_ref()
			.and_then(|revision| revision.commit.as_ref())
			.and_then(|commit| commit.url.as_deref()),
		Some("https://codeberg.org/org/monochange/commit/abc1234567890")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn prepare_release_execution_materializes_configured_lockfile_commands() {
	let fixture = monochange_test_helpers::fs::setup_fixture_from(
		env!("CARGO_MANIFEST_DIR"),
		"monochange/cargo-lock-release",
	);
	make_executable(&fixture.path().join("tools/bin/cargo"));
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		r#"

[[ecosystems.cargo.lockfile_commands]]
command = "tools/bin/cargo"
cwd = "."
"#,
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let prepared = prepare_release_execution_with_file_diffs(fixture.path(), false, false, false)
		.await
		.unwrap_or_else(|error| panic!("prepare release: {error}"));

	assert!(
		prepared
			.phase_timings
			.iter()
			.any(|phase| phase.label == "materialize lockfile command updates")
	);
	assert_eq!(
		fs::read_to_string(fixture.path().join("Cargo.lock"))
			.unwrap_or_else(|error| panic!("read Cargo.lock: {error}"))
			.trim(),
		"[[package]]\nname = \"workflow-core\"\nversion = \"1.1.0\""
	);
}

#[test]
fn dependency_sync_updates_report_manifest_parse_errors() {
	let root = PathBuf::from("/workspace");
	let core = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Deno,
		"core",
		root.join("core/deno.json"),
		root.clone(),
		Some(semver::Version::parse("1.0.0").unwrap()),
		monochange_core::PublishState::Public,
	);
	let app = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Deno,
		"app",
		root.join("app/deno.json"),
		root.clone(),
		Some(semver::Version::parse("1.0.0").unwrap()),
		monochange_core::PublishState::Public,
	);
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root.clone(),
		packages: vec![core, app],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let plan = monochange_core::ReleasePlan {
		workspace_root: root.clone(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: "deno:core/deno.json".to_string(),
			trigger_type: "direct-change".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Minor,
			planned_version: Some(semver::Version::parse("1.1.0").unwrap()),
			group_id: None,
			reasons: Vec::new(),
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let updates = vec![
		FileUpdate {
			path: root.join("core/deno.json"),
			content: br#"{"name":"core","version":"1.1.0"}"#.to_vec(),
		},
		FileUpdate {
			path: root.join("app/deno.json"),
			content: b"{".to_vec(),
		},
	];

	let error = build_dependency_sync_updates(&root, &discovery, &plan, &updates).unwrap_err();

	assert!(
		error
			.to_string()
			.contains("failed to parse deno json for sync"),
		"error: {error}"
	);
}

fn test_package(
	root: &std::path::Path,
	name: &str,
	version: &str,
) -> monochange_core::PackageRecord {
	monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		name,
		root.join(format!("crates/{name}/Cargo.toml")),
		root.to_path_buf(),
		Some(semver::Version::parse(version).unwrap()),
		monochange_core::PublishState::Public,
	)
}

#[test]
fn grouped_no_changeset_prerelease_plan_updates_group_and_state() {
	let root = std::path::PathBuf::from("/workspace");
	let mut member = test_package(&root, "member", "1.2.3");
	member.version_group_id = Some("suite".to_string());
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root.clone(),
		packages: vec![member],
		dependencies: Vec::new(),
		version_groups: vec![monochange_core::VersionGroup {
			group_id: "suite".to_string(),
			display_name: "Suite".to_string(),
			members: vec!["cargo:crates/member/Cargo.toml".to_string()],
			member_max_bumps: std::collections::BTreeMap::new(),
			mismatch_detected: false,
		}],
		warnings: Vec::new(),
	};
	let mut plan = synthesize_no_changeset_prerelease_plan(&discovery);
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		..Default::default()
	};

	apply_prerelease_versions_to_plan(&mut plan, &discovery, None, &config).unwrap();
	assert_eq!(plan.groups.len(), 1);
	assert_eq!(
		plan.groups[0].planned_version.as_ref().unwrap().to_string(),
		"1.2.3-alpha.0"
	);
	assert_eq!(
		plan.decisions[0].planned_version,
		plan.groups[0].planned_version
	);

	let prepared =
		build_prerelease_state_update(&root, &plan, &discovery, None, &config, &[]).unwrap();
	let state: serde_json::Value = serde_json::from_slice(&prepared.state_update.content).unwrap();
	assert_eq!(
		state["groups"]["suite"]["latest_prerelease_version"],
		serde_json::json!("1.2.3-alpha.0")
	);
}

// -- seed_versions_from_tag_list --
#[rstest::rstest]
#[case::package("go/custom-tag-baseline/workspace")]
#[case::group("go/custom-group-tag-baseline/workspace")]
fn seed_custom_tag_baseline_uses_configured_owner_instead_of_native_name(#[case] fixture: &str) {
	let root = monochange_test_helpers::fixture_path!(fixture);
	let configuration =
		load_workspace_configuration(&root).expect("custom tag fixture configuration");
	let mut discovery = crate::discover_workspace(&root).expect("custom tag fixture discovery");
	let tag_path = root.parent().expect("scenario directory").join("tags.txt");
	let tags = std::fs::read_to_string(tag_path).expect("read tag fixture");
	let tags = tags.lines().map(str::to_string).collect::<Vec<_>>();
	seed_versions_from_tag_list(&configuration, &mut discovery, &tags);

	assert_eq!(
		discovery
			.packages
			.first()
			.expect("Go module")
			.current_version,
		Some(semver::Version::new(1, 10, 0))
	);
	assert!(discovery.warnings.is_empty());
}

fn go_release_configuration(root: &std::path::Path, body: &str) -> WorkspaceConfiguration {
	std::fs::write(root.join("go.mod"), "module example.com/api\n\ngo 1.22\n")
		.unwrap_or_else(|error| panic!("write go.mod: {error}"));
	std::fs::write(root.join("monochange.toml"), body)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
	load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"))
}

fn go_release_record(config_id: Option<&str>) -> monochange_core::PackageRecord {
	let mut record = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Go,
		"api",
		std::path::PathBuf::from("go.mod"),
		std::path::PathBuf::from("."),
		None,
		monochange_core::PublishState::Public,
	);

	if let Some(config_id) = config_id {
		record
			.metadata
			.insert("config_id".to_string(), config_id.to_string());
	}

	record
}

fn discovery_with_packages(packages: Vec<monochange_core::PackageRecord>) -> DiscoveryReport {
	DiscoveryReport {
		workspace_root: std::path::PathBuf::from("."),
		packages,
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	}
}

#[test]
fn seed_versions_from_tag_list_warns_when_no_tag_matches() {
	let fixture = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = go_release_configuration(
		fixture.path(),
		"[package.\"go:go.mod\"]\npath = \".\"\ntype = \"go\"\nversion_format = \"primary\"\ntag = true\n",
	);
	let mut discovery = discovery_with_packages(vec![go_release_record(None)]);

	seed_versions_from_tag_list(
		&configuration,
		&mut discovery,
		&["other/v1.0.0".to_string()],
	);

	assert!(discovery.packages[0].current_version.is_none());
	assert_eq!(discovery.warnings.len(), 1);
	assert!(
		discovery.warnings[0]
			.contains("no release tag matching `v<version>` found for package `go:go.mod`")
	);
}

fn github_actions_definition(id: &str, path: &str) -> monochange_core::PackageDefinition {
	monochange_core::PackageDefinition {
		id: id.to_string(),
		path: std::path::PathBuf::from(path),
		package_type: monochange_core::PackageType::GitHubActions,
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
		version_source: monochange_core::VersionSource::Tag,
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
fn load_configured_github_actions_package_defaults_manifest_name_without_action_file() {
	let fixture = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let definition = github_actions_definition("actions", ".");

	let record =
		load_configured_github_actions_package(fixture.path(), fixture.path(), &definition)
			.unwrap_or_else(|error| panic!("load github actions package: {error}"));

	assert!(record.manifest_path.ends_with("action.yml"));
	assert_eq!(record.name, "actions");
}

/// A release record in the committed on-disk shape.
///
/// `values` and `labels` are omitted when the flags are false, which covers the
/// record written before release values existed.
fn frozen_record_json(with_values: bool, with_labels: bool) -> String {
	let mut record = serde_json::json!({
		"changedFiles": [],
		"changelogs": [],
		"changesets": [],
		"command": "release",
		"createdAt": "2026-09-19T12:00:00Z",
		"kind": "monochange.releaseRecord",
		"schemaVersion": "0.7",
		"packagePublications": [],
		"releaseTargets": [],
		"releasedPackages": ["app"],
		"updatedChangelogs": [],
		"deletedChangesets": [],
	});

	if with_values {
		record["values"] = serde_json::json!({"app.build": "5"});
	}

	if with_labels {
		record["labels"] = serde_json::json!({"app": "2026.9.1"});
		record["labelInputs"] = serde_json::json!({
			"date": "2026-09-19",
			"time": "120000",
			"ofMonth": 1,
			"ofQuarter": 1,
			"ofYear": 1
		});
	}

	record.to_string()
}

fn versioning_plan(root: &Path, package_id: &str, version: &str) -> monochange_core::ReleasePlan {
	monochange_core::ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: package_id.to_string(),
			trigger_type: "direct-change".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Minor,
			planned_version: Some(
				semver::Version::parse(version).unwrap_or_else(|error| panic!("version: {error}")),
			),
			group_id: None,
			reasons: Vec::new(),
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	}
}

#[tokio::test]
async fn resolve_release_values_for_prepare_reuses_a_frozen_record() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	// The plan is keyed by discovery record id while declared values live under
	// the configured package id, so the record carries both.
	let mut package = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Npm,
		"app",
		root.join("package.json"),
		root.to_path_buf(),
		Some(semver::Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), "app".to_string());
	let record_id = package.id.clone();
	let plan = versioning_plan(root, &record_id, "1.1.0");
	let packages = vec![package];

	// The record is written for the identity the plan resolves to, so the
	// idempotency lookup finds it and returns the values unchanged.
	let targets = vec![monochange_core::ReleaseManifestTarget {
		id: record_id.clone(),
		kind: monochange_core::ReleaseOwnerKind::Package,
		version: "1.1.0".to_string(),
		tag: false,
		release: false,
		version_format: monochange_core::VersionFormat::default(),
		tag_name: String::new(),
		members: Vec::new(),
		rendered_title: String::new(),
		rendered_changelog_title: String::new(),
		floating_tags: Vec::new(),
	}];
	let paths = release_record_paths(root, &targets);
	std::fs::create_dir_all(
		paths
			.absolute
			.parent()
			.unwrap_or_else(|| panic!("record parent")),
	)
	.unwrap_or_else(|error| panic!("create record dir: {error}"));
	fs::write(&paths.absolute, frozen_record_json(true, true))
		.unwrap_or_else(|error| panic!("write record: {error}"));

	let configuration = workspace_configuration_with_lockfile_commands();
	let values = resolve_release_values_for_prepare(root, &configuration, &packages, &plan)
		.await
		.unwrap_or_else(|error| panic!("resolve values: {error}"));

	assert_eq!(
		values.values_for("app").get("build"),
		Some(&"5".to_string())
	);
}

#[test]
fn ecosystem_discovery_settings_filter_discovered_packages() {
	use monochange_core::filter_discovered_packages_by_ecosystem_settings;

	fn package(manifest: &str) -> monochange_core::PackageRecord {
		monochange_core::PackageRecord::new(
			monochange_core::Ecosystem::Cargo,
			"pkg",
			PathBuf::from(manifest),
			PathBuf::from(manifest),
			None,
			monochange_core::PublishState::Public,
		)
	}

	fn configuration(settings: monochange_core::EcosystemSettings) -> WorkspaceConfiguration {
		let mut configuration = workspace_configuration_with_lockfile_commands();
		configuration.cargo = settings;
		configuration
	}

	let root = Path::new("/repo");
	let inside = package("/repo/crates/core/Cargo.toml");
	let outside = package("/repo/tools/helper/Cargo.toml");
	let mut warnings = Vec::new();

	// Defaults keep everything.
	let defaults = monochange_core::EcosystemSettings::default();
	let kept = filter_discovered_packages_by_ecosystem_settings(
		root,
		&configuration(defaults.clone()),
		vec![inside.clone(), outside.clone()],
		&mut warnings,
	);
	assert_eq!(kept.len(), 2, "default settings keep every package");

	// roots restricts discovery to packages under the listed directories.
	let mut rooted = defaults.clone();
	rooted.roots = vec!["crates".to_string()];
	let kept = filter_discovered_packages_by_ecosystem_settings(
		root,
		&configuration(rooted),
		vec![inside.clone(), outside.clone()],
		&mut warnings,
	);
	assert_eq!(
		kept.len(),
		1,
		"roots drop packages outside the listed directories"
	);
	assert_eq!(kept[0].manifest_path, inside.manifest_path);

	// exclude removes packages whose manifest matches the glob.
	let mut excluded = defaults.clone();
	excluded.exclude = vec!["tools/**".to_string()];
	let kept = filter_discovered_packages_by_ecosystem_settings(
		root,
		&configuration(excluded),
		vec![inside.clone(), outside.clone()],
		&mut warnings,
	);
	assert_eq!(kept.len(), 1, "exclude globs remove matching manifests");
	assert_eq!(kept[0].manifest_path, inside.manifest_path);

	// enabled = false drops the whole ecosystem.
	let mut disabled = defaults;
	disabled.enabled = Some(false);
	let kept = filter_discovered_packages_by_ecosystem_settings(
		root,
		&configuration(disabled),
		vec![inside, outside],
		&mut warnings,
	);
	assert!(kept.is_empty(), "enabled = false drops the ecosystem");
	assert!(
		warnings.is_empty(),
		"valid settings never warn: {warnings:?}"
	);
}
