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
use monochange_core::WorkspaceConfiguration;

use super::*;

#[test]
fn render_step_inputs_toml_uses_array_for_inherited_and_map_for_mixed_inputs() {
	let mut inherited_inputs = BTreeMap::new();
	inherited_inputs.insert(
		"format".to_string(),
		monochange_core::CliStepInputValue::Inherited,
	);
	let mut rendered = String::new();
	render_step_inputs_toml(&mut rendered, &inherited_inputs);
	assert_eq!(rendered, "inputs = [\"format\"]\n");

	inherited_inputs.insert(
		"release".to_string(),
		monochange_core::CliStepInputValue::Inherited,
	);
	let mut rendered = String::new();
	render_step_inputs_toml(&mut rendered, &inherited_inputs);
	assert_eq!(rendered, "inputs = [\"format\", \"release\"]\n");

	inherited_inputs.insert(
		"draft".to_string(),
		monochange_core::CliStepInputValue::Boolean(true),
	);
	let rendered = render_step_inputs_inline_table(&inherited_inputs);
	assert_eq!(
		rendered,
		"{ draft = true, format = \"{{ inputs.format }}\", release = \"{{ inputs.release }}\" }"
	);
}

#[test]
fn write_toml_array_items_streams_values_without_outer_brackets() {
	let mut rendered = String::new();
	write_toml_array_items(
		&mut rendered,
		["core".to_string(), "web app".to_string()].iter(),
	);

	assert_eq!(rendered, "\"core\", \"web app\"");
}

#[test]
fn render_command_variables_inline_table_streams_multiple_variables() {
	let mut variables = BTreeMap::new();
	variables.insert(
		"changed".to_string(),
		monochange_core::CommandVariable::ChangedFiles,
	);
	variables.insert(
		"version".to_string(),
		monochange_core::CommandVariable::Version,
	);

	assert_eq!(
		render_command_variables_inline_table(&variables),
		"{ changed = \"changed_files\", version = \"version\" }"
	);
}

fn setup_workspace_ops_fixture() -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(
		env!("CARGO_MANIFEST_DIR"),
		"workspace-ops/lockfile-command-helpers",
	)
}

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
	}
}

#[test]
fn remap_workspace_path_rejects_paths_outside_the_workspace() {
	let fixture = setup_workspace_ops_fixture();
	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = remap_workspace_path(
		fixture.path(),
		temp_root.path(),
		Path::new("/tmp/outside-workspace"),
	)
	.err()
	.unwrap_or_else(|| panic!("expected remap error"));
	assert!(error.to_string().contains("was outside workspace root"));
}

#[test]
fn remap_workspace_path_maps_workspace_paths_into_the_temp_root() {
	let fixture = setup_workspace_ops_fixture();
	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	let remapped = remap_workspace_path(
		fixture.path(),
		temp_root.path(),
		&fixture.path().join("root.txt"),
	)
	.unwrap_or_else(|error| panic!("remap workspace path: {error}"));

	assert_eq!(remapped, temp_root.path().join("root.txt"));
}

#[test]
fn init_and_populate_workspace_cover_common_error_and_noop_paths() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("monochange.toml"))
		.unwrap_or_else(|error| panic!("create blocking config dir: {error}"));
	let init_error = init_workspace(root, true, None)
		.err()
		.unwrap_or_else(|| panic!("expected init write error"));
	assert!(init_error.to_string().contains("failed to write"));

	let empty_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing_error = populate_workspace(empty_root.path())
		.err()
		.unwrap_or_else(|| panic!("expected missing config error"));
	assert!(missing_error.to_string().contains("run `mc init` first"));

	let populated_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		populated_root.path().join("monochange.toml"),
		render_cli_commands_toml(&default_cli_commands()),
	)
	.unwrap_or_else(|error| panic!("write populated config: {error}"));
	let populated = populate_workspace(populated_root.path())
		.unwrap_or_else(|error| panic!("populate no-op: {error}"));
	assert!(populated.added_commands.is_empty());
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
	assert!(rendered.contains("type = \"flutter\""));
	assert!(rendered.contains("changelog = \"packages/core/changelog.md\""));
	assert!(rendered.contains("packages/core"));

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
	assert_eq!(
		package_type_for_ecosystem(Ecosystem::Dart),
		PackageType::Flutter
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
fn run_lockfile_command_reports_parse_spawn_and_exit_failures() {
	let fixture = setup_workspace_ops_fixture();
	for script in ["fail-no-stderr", "fail-stderr"] {
		make_executable(&fixture.path().join("tools/bin").join(script));
	}
	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_workspace_tree(fixture.path(), temp_root.path())
		.unwrap_or_else(|error| panic!("copy workspace: {error}"));
	for script in ["fail-no-stderr", "fail-stderr"] {
		make_executable(&temp_root.path().join("tools/bin").join(script));
	}

	let parse_error = run_lockfile_command(
		fixture.path(),
		temp_root.path(),
		&LockfileCommandExecution {
			command: "'".to_string(),
			cwd: fixture.path().to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected parse error"));
	assert!(parse_error.to_string().contains("failed to parse command"));

	let empty_error = run_lockfile_command(
		fixture.path(),
		temp_root.path(),
		&LockfileCommandExecution {
			command: "   ".to_string(),
			cwd: fixture.path().to_path_buf(),
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

	let spawn_error = run_lockfile_command(
		fixture.path(),
		temp_root.path(),
		&LockfileCommandExecution {
			command: "definitely-not-a-real-command".to_string(),
			cwd: fixture.path().to_path_buf(),
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

	let no_stderr_error = run_lockfile_command(
		fixture.path(),
		temp_root.path(),
		&LockfileCommandExecution {
			command: temp_root
				.path()
				.join("tools/bin/fail-no-stderr")
				.display()
				.to_string(),
			cwd: fixture.path().to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected nonzero exit error"));
	assert!(no_stderr_error.to_string().contains("exit status"));

	let stderr_error = run_lockfile_command(
		fixture.path(),
		temp_root.path(),
		&LockfileCommandExecution {
			command: temp_root
				.path()
				.join("tools/bin/fail-stderr")
				.display()
				.to_string(),
			cwd: fixture.path().to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected stderr error"));
	assert!(stderr_error.to_string().contains("bad stderr"));
}

#[test]
fn workspace_copy_and_diff_helpers_skip_git_and_capture_new_files() {
	let fixture = setup_workspace_ops_fixture();
	make_executable(&fixture.path().join("tools/bin/write-new-file"));
	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_workspace_tree(fixture.path(), temp_root.path())
		.unwrap_or_else(|error| panic!("copy workspace: {error}"));
	make_executable(&temp_root.path().join("tools/bin/write-new-file"));
	assert!(temp_root.path().join("root.txt").exists());
	assert!(!temp_root.path().join(".git").exists());

	run_lockfile_command(
		fixture.path(),
		temp_root.path(),
		&LockfileCommandExecution {
			command: temp_root
				.path()
				.join("tools/bin/write-new-file")
				.display()
				.to_string(),
			cwd: fixture.path().to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.unwrap_or_else(|error| panic!("write generated file: {error}"));

	let lockfile_cmd = LockfileCommandExecution {
		command: String::new(),
		cwd: fixture.path().to_path_buf(),
		shell: ShellConfig::None,
	};
	let updates =
		collect_workspace_file_updates(fixture.path(), temp_root.path(), &[], &[lockfile_cmd])
			.unwrap_or_else(|error| panic!("collect updates: {error}"));
	assert!(
		updates
			.iter()
			.any(|update| update.path.ends_with("generated.txt"))
	);

	let mut paths = BTreeSet::new();
	collect_workspace_files(fixture.path(), fixture.path(), &mut paths)
		.unwrap_or_else(|error| panic!("collect files: {error}"));
	assert!(paths.contains(Path::new("root.txt")));
	assert!(!paths.iter().any(|path| path.starts_with(".git")));
}

#[test]
fn file_helpers_report_missing_and_invalid_paths() {
	let fixture = setup_workspace_ops_fixture();
	assert!(
		read_optional_file(&fixture.path().join("missing.txt"))
			.unwrap_or_else(|error| panic!("missing file lookup: {error}"))
			.is_none()
	);
	let read_error = read_optional_file(fixture.path())
		.err()
		.unwrap_or_else(|| panic!("expected directory read error"));
	assert!(read_error.to_string().contains("failed to read"));

	let collect_error = collect_workspace_files(
		fixture.path(),
		&fixture.path().join("missing-dir"),
		&mut BTreeSet::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected collect files error"));
	assert!(collect_error.to_string().contains("failed to read"));
	let strip_prefix_error = strip_workspace_prefix(
		&fixture.path().join("root.txt"),
		Path::new("/tmp/other-root"),
	)
	.err()
	.unwrap_or_else(|| panic!("expected strip-prefix error"));
	assert!(
		strip_prefix_error
			.to_string()
			.contains("was outside workspace root")
	);

	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination_file = temp_root.path().join("not-a-directory");
	fs::write(&destination_file, "file")
		.unwrap_or_else(|error| panic!("write destination file: {error}"));
	let copy_error = copy_workspace_tree(fixture.path(), &destination_file)
		.err()
		.unwrap_or_else(|| panic!("expected copy workspace error"));
	assert!(copy_error.to_string().contains("failed to create"));
	let missing_source_error =
		copy_workspace_tree(&fixture.path().join("missing-source"), temp_root.path())
			.err()
			.unwrap_or_else(|| panic!("expected missing source error"));
	assert!(missing_source_error.to_string().contains("failed to read"));

	let destination_dir = temp_root.path().join("destination-dir");
	fs::create_dir_all(&destination_dir)
		.unwrap_or_else(|error| panic!("create destination dir: {error}"));
	let source_root = temp_root.path().join("copy-source");
	fs::create_dir_all(source_root.join("nested"))
		.unwrap_or_else(|error| panic!("create source dir: {error}"));
	fs::write(source_root.join("nested/file.txt"), "file")
		.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(destination_dir.join("nested"), "blocking file")
		.unwrap_or_else(|error| panic!("write blocking file: {error}"));
	let parent_create_error = ensure_parent_directory(&destination_dir.join("nested/file.txt"))
		.err()
		.unwrap_or_else(|| panic!("expected parent create error"));
	assert!(parent_create_error.to_string().contains("failed to create"));

	let copy_target_root = temp_root.path().join("copy-target-root");
	fs::create_dir_all(&copy_target_root)
		.unwrap_or_else(|error| panic!("create copy target root: {error}"));
	fs::create_dir_all(copy_target_root.join("root.txt"))
		.unwrap_or_else(|error| panic!("create copy target dir collision: {error}"));
	let copy_file_error = copy_workspace_file(
		&fixture.path().join("root.txt"),
		&copy_target_root.join("root.txt"),
	)
	.err()
	.unwrap_or_else(|| panic!("expected copy file error"));
	assert!(copy_file_error.to_string().contains("failed to copy"));
}

#[test]
fn snapshot_directory_files_captures_file_contents() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("pkg")).unwrap();
	fs::write(root.join("pkg/lock.json"), b"lockfile-content").unwrap();
	fs::write(root.join("pkg/other.txt"), b"other").unwrap();

	let mut snapshots = BTreeMap::new();
	snapshot_directory_files(root, &root.join("pkg"), &mut snapshots)
		.unwrap_or_else(|error| panic!("snapshot: {error}"));

	assert_eq!(snapshots.len(), 2);
	assert_eq!(
		snapshots.get(Path::new("pkg/lock.json")).map(Vec::as_slice),
		Some(b"lockfile-content".as_slice())
	);
}

#[test]
fn snapshot_directory_files_skips_subdirectories() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("pkg/subdir")).unwrap();
	fs::write(root.join("pkg/top.txt"), b"top").unwrap();
	fs::write(root.join("pkg/subdir/nested.txt"), b"nested").unwrap();

	let mut snapshots = BTreeMap::new();
	snapshot_directory_files(root, &root.join("pkg"), &mut snapshots)
		.unwrap_or_else(|error| panic!("snapshot: {error}"));

	// Only top-level files, not nested.
	assert_eq!(snapshots.len(), 1);
	assert!(snapshots.contains_key(Path::new("pkg/top.txt")));
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
fn materialize_lockfile_updates_captures_in_place_changes() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	fs::create_dir_all(root.join("tools/bin")).unwrap();
	let script_path = root.join("tools/bin/update-lock");
	fs::write(
		&script_path,
		"#!/bin/sh\necho 'updated-lockfile' > \"$PWD/lock.txt\"\n",
	)
	.unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
	}

	fs::write(root.join("lock.txt"), b"original").unwrap();
	fs::write(root.join("manifest.json"), b"old-version").unwrap();

	let base_updates = vec![FileUpdate {
		path: root.join("manifest.json"),
		content: b"new-version".to_vec(),
	}];
	let lockfile_commands = vec![LockfileCommandExecution {
		command: script_path.display().to_string(),
		cwd: root.to_path_buf(),
		shell: ShellConfig::None,
	}];

	let updates = materialize_lockfile_command_updates(root, &base_updates, &lockfile_commands)
		.unwrap_or_else(|error| panic!("materialize: {error}"));

	assert!(
		updates.iter().any(|u| u.path.ends_with("manifest.json")),
		"expected manifest update"
	);
	assert!(
		updates.iter().any(|u| u.path.ends_with("lock.txt")),
		"expected lockfile update"
	);
	let lock_content = fs::read_to_string(root.join("lock.txt")).unwrap();
	assert!(
		lock_content.contains("updated-lockfile"),
		"lockfile should be updated in-place"
	);
}

#[test]
fn lockfile_update_helpers_cover_missing_dirs_unreadable_files_and_stderr_paths() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let unreadable_dir = root.join("snapshots");
	fs::create_dir_all(&unreadable_dir).unwrap();
	let unreadable_file = unreadable_dir.join("blocked.txt");
	fs::write(&unreadable_file, "blocked").unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o000)).unwrap();
	}
	let snapshot_error = snapshot_directory_files(root, &unreadable_dir, &mut BTreeMap::new())
		.err()
		.unwrap_or_else(|| panic!("expected unreadable snapshot file error"));
	assert!(snapshot_error.to_string().contains("failed to read"));
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o644)).unwrap();
	}

	fs::create_dir_all(root.join("tools/bin")).unwrap();
	let script_path = root.join("tools/bin/fail-with-stderr");
	fs::write(&script_path, "#!/bin/sh\necho 'bad stderr' >&2\nexit 4\n").unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
	}
	let stderr_error = run_lockfile_command_in_place(
		root,
		&LockfileCommandExecution {
			command: script_path.display().to_string(),
			cwd: PathBuf::from("."),
			shell: ShellConfig::None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected stderr failure for in-place lockfile command"));
	assert!(stderr_error.to_string().contains("bad stderr"));
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
}

#[test]
fn run_lockfile_command_variants_cover_success_and_shell_paths() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("tools/bin"))
		.unwrap_or_else(|error| panic!("create tools dir: {error}"));
	let success_script = root.join("tools/bin/succeed");
	fs::write(
		&success_script,
		"#!/bin/sh\necho shell-success > \"$PWD/shell-output.txt\"\n",
	)
	.unwrap_or_else(|error| panic!("write success script: {error}"));
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&success_script, fs::Permissions::from_mode(0o755))
			.unwrap_or_else(|error| panic!("chmod success script: {error}"));
	}

	run_lockfile_command_in_place(
		root,
		&LockfileCommandExecution {
			command: format!("sh {}", success_script.display()),
			cwd: root.to_path_buf(),
			shell: ShellConfig::None,
		},
	)
	.unwrap_or_else(|error| panic!("run in-place direct command: {error}"));
	assert!(root.join("shell-output.txt").exists());

	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_workspace_tree(root, temp_root.path())
		.unwrap_or_else(|error| panic!("copy workspace for test command: {error}"));
	let remapped_script = temp_root.path().join("tools/bin/succeed");
	run_lockfile_command(
		root,
		temp_root.path(),
		&LockfileCommandExecution {
			command: format!("sh {}", remapped_script.display()),
			cwd: root.to_path_buf(),
			shell: ShellConfig::Default,
		},
	)
	.unwrap_or_else(|error| panic!("run workspace lockfile command through shell: {error}"));
	assert!(temp_root.path().join("shell-output.txt").exists());
}

#[test]
fn collect_workspace_file_updates_ignores_parentless_base_updates() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("root tempdir: {error}"));
	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let base_update = FileUpdate {
		path: PathBuf::new(),
		content: Vec::new(),
	};

	let updates =
		collect_workspace_file_updates(root.path(), temp_root.path(), &[base_update], &[])
			.unwrap_or_else(|error| panic!("collect updates: {error}"));

	assert!(updates.is_empty());
}

#[test]
fn collect_workspace_file_updates_handles_outside_command_cwds() {
	let fixture = setup_workspace_ops_fixture();
	let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_workspace_tree(fixture.path(), temp_root.path())
		.unwrap_or_else(|error| panic!("copy workspace: {error}"));

	let updates = collect_workspace_file_updates(
		fixture.path(),
		temp_root.path(),
		&[FileUpdate {
			path: fixture.path().join("Cargo.toml"),
			content: b"[workspace]\n".to_vec(),
		}],
		&[LockfileCommandExecution {
			command: "echo ignored".to_string(),
			cwd: PathBuf::from("/tmp/outside-workspace"),
			shell: ShellConfig::None,
		}],
	)
	.unwrap_or_else(|error| panic!("collect workspace file updates: {error}"));
	assert!(
		updates
			.iter()
			.all(|update| update.path.starts_with(fixture.path()))
	);
}

#[test]
fn read_and_delete_changeset_helpers_report_missing_paths() {
	let error = read_changeset_source(Path::new("missing/changeset.md"))
		.err()
		.unwrap_or_else(|| panic!("expected read error"));
	assert!(
		error
			.to_string()
			.contains("failed to read missing/changeset.md")
	);

	let error = delete_changeset_file(Path::new("missing/changeset.md"))
		.err()
		.unwrap_or_else(|| panic!("expected delete error"));
	assert!(
		error
			.to_string()
			.contains("failed to delete missing/changeset.md")
	);
}

#[test]
fn changeset_context_phase_label_covers_annotate_and_enrich_modes() {
	let gitlab = sample_source(SourceProvider::GitLab);
	let github = sample_source(SourceProvider::GitHub);
	assert_eq!(github.host, None);
	assert_eq!(
		changeset_context_phase_label(&gitlab, true),
		"annotate changeset context via gitlab"
	);
	assert_eq!(
		changeset_context_phase_label(&gitlab, false),
		"enrich changeset context via gitlab"
	);
}

#[test]
fn changeset_context_timeout_uses_configured_source_release_timeout() {
	let mut source = sample_source(SourceProvider::GitHub);
	assert_eq!(changeset_context_timeout(&source), Duration::from_secs(120));

	source.releases.changeset_context_timeout_seconds = 7;
	assert_eq!(changeset_context_timeout(&source), Duration::from_secs(7));
}

#[test]
fn warn_about_incomplete_cargo_lockfiles_returns_early_when_commands_are_configured() {
	let configuration = workspace_configuration_with_lockfile_commands();
	warn_about_incomplete_cargo_lockfiles(Path::new("."), &configuration, &[], &BTreeMap::new());
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

#[tokio::test(flavor = "multi_thread")]
async fn prepare_release_execution_tracks_gitlab_context_phase_timing() {
	let fixture = monochange_test_helpers::fs::setup_fixture_from(
		env!("CARGO_MANIFEST_DIR"),
		"monochange/release-base",
	);
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		r#"

[source]
provider = "gitlab"
owner = "group"
repo = "monochange"
"#,
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let prepared = prepare_release_execution_with_file_diffs(fixture.path(), true, false, false)
		.await
		.unwrap_or_else(|error| panic!("prepare release: {error}"));

	assert!(
		prepared
			.phase_timings
			.iter()
			.any(|phase| phase.label == "annotate changeset context via gitlab")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn prepare_release_execution_tracks_github_background_context_phase_timing() {
	let fixture = monochange_test_helpers::fs::setup_fixture_from(
		env!("CARGO_MANIFEST_DIR"),
		"monochange/release-base",
	);
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		r#"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"
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
			.any(|phase| phase.label == "enrich changeset context via github")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_changeset_context_enrichment_with_timeout_reports_completion() {
	let source = sample_source(SourceProvider::GitHub);
	let completed =
		run_changeset_context_enrichment_with_timeout(&source, Duration::from_secs(1), async {})
			.await;

	assert!(completed);
}

#[tokio::test(flavor = "current_thread")]
async fn run_changeset_context_enrichment_with_timeout_returns_false_on_elapsed_timeout() {
	let subscriber = tracing_subscriber::fmt()
		.with_max_level(tracing::Level::WARN)
		.with_writer(std::io::sink)
		.finish();
	let _default_subscriber = tracing::subscriber::set_default(subscriber);
	let source = sample_source(SourceProvider::GitHub);
	let completed = run_changeset_context_enrichment_with_timeout(
		&source,
		Duration::from_millis(1),
		std::future::pending::<()>(),
	)
	.await;

	assert!(!completed);
}

struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for DropSignal {
	fn drop(&mut self) {
		if let Some(sender) = self.0.take() {
			let _ = sender.send(());
		}
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn source_changeset_context_task_aborts_background_work_when_dropped() {
	let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
	let (dropped_sender, dropped_receiver) = tokio::sync::oneshot::channel();
	let handle = tokio::spawn(async move {
		let _ = started_sender.send(());
		let _drop_signal = DropSignal(Some(dropped_sender));
		std::future::pending::<(Vec<PreparedChangeset>, StepPhaseTiming)>().await
	});
	let task = SourceChangesetContextTask::new(handle);

	started_receiver
		.await
		.unwrap_or_else(|error| panic!("background task did not start: {error}"));
	drop(task);
	let drop_result = tokio::time::timeout(Duration::from_secs(1), dropped_receiver).await;

	assert!(
		matches!(drop_result, Ok(Ok(()))),
		"background task was not dropped: {drop_result:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn join_source_changeset_context_task_reports_background_panic() {
	let mut phase_timings = Vec::new();
	let handle = tokio::spawn(async {
		panic!("boom");
	});
	let task = SourceChangesetContextTask::new(handle);

	let error = join_source_changeset_context_task(&mut phase_timings, task)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected join error"));

	assert!(
		error
			.to_string()
			.contains("background changeset context enrichment panicked"),
		"error: {error}"
	);
	assert!(phase_timings.is_empty());
}

#[test]
fn prerelease_planned_mode_reuses_original_stable_from_state() {
	let root = std::path::PathBuf::from("/workspace");
	let package = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(semver::Version::parse("2.0.0-alpha.0").unwrap()),
		monochange_core::PublishState::Public,
	);
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root,
		packages: vec![package],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let mut previous = PrereleaseState {
		schema_version: PRERELEASE_STATE_SCHEMA_VERSION,
		channel: "alpha".to_string(),
		numbering: monochange_core::PrereleaseNumbering::Increment,
		base: monochange_core::PrereleaseBase::Planned,
		created_at: "2026-05-22T00:00:00Z".to_string(),
		updated_at: "2026-05-22T00:00:00Z".to_string(),
		packages: std::collections::BTreeMap::new(),
		groups: std::collections::BTreeMap::new(),
	};
	previous.packages.insert(
		"cargo:crates/core/Cargo.toml".to_string(),
		PrereleaseStateEntry {
			original_stable: semver::Version::parse("1.0.0").unwrap(),
			planned_stable: semver::Version::parse("2.0.0").unwrap(),
			latest_prerelease: semver::Version::parse("2.0.0-alpha.0").unwrap(),
		},
	);

	let adjusted = discovery_with_prerelease_baselines(
		&discovery,
		Some(&previous),
		&monochange_core::PrereleaseConfiguration::default(),
	);

	assert_eq!(
		adjusted.packages[0].current_version,
		Some(semver::Version::parse("1.0.0").unwrap())
	);
}

#[test]
fn prerelease_versions_increment_within_stable_base() {
	let mut plan = monochange_core::ReleasePlan {
		workspace_root: std::path::PathBuf::from("/workspace"),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: "cargo:crates/core/Cargo.toml".to_string(),
			trigger_type: "direct-change".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Major,
			planned_version: Some(semver::Version::parse("2.0.0").unwrap()),
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
	let root = std::path::PathBuf::from("/workspace");
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root.clone(),
		packages: vec![monochange_core::PackageRecord::new(
			monochange_core::Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root,
			Some(semver::Version::parse("1.0.0").unwrap()),
			monochange_core::PublishState::Public,
		)],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let mut previous = PrereleaseState {
		schema_version: PRERELEASE_STATE_SCHEMA_VERSION,
		channel: "alpha".to_string(),
		numbering: monochange_core::PrereleaseNumbering::Increment,
		base: monochange_core::PrereleaseBase::Planned,
		created_at: "2026-05-22T00:00:00Z".to_string(),
		updated_at: "2026-05-22T00:00:00Z".to_string(),
		packages: std::collections::BTreeMap::new(),
		groups: std::collections::BTreeMap::new(),
	};
	previous.packages.insert(
		"cargo:crates/core/Cargo.toml".to_string(),
		PrereleaseStateEntry {
			original_stable: semver::Version::parse("1.0.0").unwrap(),
			planned_stable: semver::Version::parse("2.0.0").unwrap(),
			latest_prerelease: semver::Version::parse("2.0.0-alpha.0").unwrap(),
		},
	);
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		..Default::default()
	};

	apply_prerelease_versions_to_plan(&mut plan, &discovery, Some(&previous), &config).unwrap();

	assert_eq!(
		plan.decisions[0].planned_version,
		Some(semver::Version::parse("2.0.0-alpha.1").unwrap())
	);
}

#[test]
fn fixed_prerelease_base_overrides_planned_version() {
	let mut plan = monochange_core::ReleasePlan {
		workspace_root: std::path::PathBuf::from("/workspace"),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: "cargo:crates/core/Cargo.toml".to_string(),
			trigger_type: "direct-change".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Major,
			planned_version: Some(semver::Version::parse("2.0.0").unwrap()),
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
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: std::path::PathBuf::from("/workspace"),
		packages: Vec::new(),
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let config = monochange_core::PrereleaseConfiguration {
		base: monochange_core::PrereleaseBase::Fixed,
		base_version: Some(semver::Version::parse("0.0.0").unwrap()),
		..Default::default()
	};

	apply_prerelease_versions_to_plan(&mut plan, &discovery, None, &config).unwrap();

	assert_eq!(
		plan.decisions[0].planned_version,
		Some(semver::Version::parse("0.0.0-alpha.0").unwrap())
	);
}

#[test]
fn prerelease_state_loads_missing_valid_and_malformed_json() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	assert!(load_prerelease_state(root).unwrap().is_none());

	let state_path = prerelease_state_path(root);
	std::fs::create_dir_all(state_path.parent().unwrap())
		.unwrap_or_else(|error| panic!("state dir: {error}"));
	std::fs::write(
		&state_path,
		serde_json::json!({
			"schema_version": 1,
			"channel": "alpha",
			"numbering": "increment",
			"base": "planned",
			"created_at": "2026-05-22T00:00:00Z",
			"updated_at": "2026-05-22T00:00:00Z",
			"packages": {
				"cargo:crates/core/Cargo.toml": {
					"original_stable_version": "1.0.0",
					"planned_stable_version": "2.0.0",
					"latest_prerelease_version": "2.0.0-alpha.1"
				}
			},
			"groups": {}
		})
		.to_string(),
	)
	.unwrap_or_else(|error| panic!("write state: {error}"));

	let loaded = load_prerelease_state(root)
		.unwrap()
		.unwrap_or_else(|| panic!("expected state"));
	let package = loaded
		.packages
		.get("cargo:crates/core/Cargo.toml")
		.unwrap_or_else(|| panic!("expected package state"));
	assert_eq!(loaded.channel, "alpha");
	assert_eq!(package.original_stable, semver::Version::new(1, 0, 0));
	assert_eq!(package.latest_prerelease.to_string(), "2.0.0-alpha.1");

	std::fs::write(&state_path, b"{").unwrap_or_else(|error| panic!("write malformed: {error}"));
	let error = load_prerelease_state(root).expect_err("malformed state should fail");
	let rendered = error.to_string();
	assert!(rendered.contains("failed to parse"));
	assert!(rendered.contains("prerelease-state.json"));
}

#[test]
fn no_changeset_prerelease_plan_synthesizes_package_decisions() {
	let root = std::path::PathBuf::from("/workspace");
	let package = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(semver::Version::parse("1.2.3").unwrap()),
		monochange_core::PublishState::Public,
	);
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root,
		packages: vec![package],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let mut plan = synthesize_no_changeset_prerelease_plan(&discovery);
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		..Default::default()
	};

	apply_prerelease_versions_to_plan(&mut plan, &discovery, None, &config).unwrap();

	assert_eq!(plan.decisions.len(), 1);
	assert_eq!(plan.decisions[0].trigger_type, "prerelease");
	assert_eq!(
		plan.decisions[0].planned_version,
		Some(semver::Version::parse("1.2.3-alpha.0").unwrap())
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
fn prerelease_identifier_covers_numbering_and_mismatched_latest_base() {
	let stable = semver::Version::parse("2.0.0").unwrap();
	let previous_for_other_base = semver::Version::parse("1.0.0-alpha.4").unwrap();
	let mut config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		..Default::default()
	};

	assert_eq!(
		prerelease_identifier(&config, &stable, Some(&previous_for_other_base)),
		"alpha.0"
	);

	config.numbering = monochange_core::PrereleaseNumbering::Date;
	assert!(prerelease_identifier(&config, &stable, None).starts_with("alpha.20"));

	config.numbering = monochange_core::PrereleaseNumbering::Datetime;
	assert!(prerelease_identifier(&config, &stable, None).starts_with("alpha.20"));
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

	let prepared = build_prerelease_state_update(&root, &plan, &discovery, None, &config).unwrap();
	let state: serde_json::Value = serde_json::from_slice(&prepared.state_update.content).unwrap();
	assert_eq!(
		state["groups"]["suite"]["latest_prerelease_version"],
		serde_json::json!("1.2.3-alpha.0")
	);
}

#[test]
fn prerelease_state_rejects_unsupported_schema_version() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = prerelease_state_path(tempdir.path());
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(
		&path,
		serde_json::json!({
			"schema_version": 999,
			"channel": "alpha",
			"numbering": "increment",
			"base": "planned",
			"created_at": "2026-05-22T00:00:00Z",
			"updated_at": "2026-05-22T00:00:00Z",
			"packages": {},
			"groups": {}
		})
		.to_string(),
	)
	.unwrap();

	let error = load_prerelease_state(tempdir.path()).expect_err("unsupported schema should fail");
	assert!(error.to_string().contains("unsupported schema_version 999"));
}

#[test]
fn prerelease_state_reports_read_failures() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = prerelease_state_path(tempdir.path());
	std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("state directory: {error}"));

	let error =
		load_prerelease_state(tempdir.path()).expect_err("directory should not load as JSON");
	let rendered = error.to_string();
	assert!(rendered.contains("failed to read"), "error: {rendered}");
	assert!(
		rendered.contains("prerelease-state.json"),
		"error: {rendered}"
	);
}

#[test]
fn invalid_prerelease_channel_reports_version_build_error() {
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		channel: "alpha+bad".to_string(),
		..Default::default()
	};
	let base = semver::Version::parse("1.2.3").unwrap();

	let error = append_prerelease(&base, &config, None).expect_err("invalid channel should fail");
	assert!(
		error
			.to_string()
			.contains("failed to build prerelease version for `1.2.3`"),
		"error: {error}"
	);
}

#[test]
fn prerelease_plan_skips_unplanned_group_and_uses_current_stable_group_base() {
	let root = std::path::PathBuf::from("/workspace");
	let mut member = test_package(&root, "member", "1.2.3-alpha.7");
	member.version_group_id = Some("suite".to_string());
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root,
		packages: vec![member],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let mut plan = monochange_core::ReleasePlan {
		workspace_root: std::path::PathBuf::from("/workspace"),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: "cargo:crates/member/Cargo.toml".to_string(),
			trigger_type: "direct-change".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Patch,
			planned_version: None,
			group_id: None,
			reasons: Vec::new(),
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: vec![
			monochange_core::PlannedVersionGroup {
				group_id: "skipped".to_string(),
				display_name: "Skipped".to_string(),
				members: Vec::new(),
				mismatch_detected: false,
				planned_version: None,
				recommended_bump: monochange_core::BumpSeverity::Patch,
			},
			monochange_core::PlannedVersionGroup {
				group_id: "suite".to_string(),
				display_name: "Suite".to_string(),
				members: vec!["cargo:crates/member/Cargo.toml".to_string()],
				mismatch_detected: false,
				planned_version: Some(semver::Version::parse("2.0.0").unwrap()),
				recommended_bump: monochange_core::BumpSeverity::Patch,
			},
		],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		base: monochange_core::PrereleaseBase::CurrentStable,
		..Default::default()
	};

	apply_prerelease_versions_to_plan(&mut plan, &discovery, None, &config).unwrap();

	assert!(plan.groups[0].planned_version.is_none());
	assert_eq!(
		plan.groups[1].planned_version,
		Some(semver::Version::parse("1.2.3-alpha.0").unwrap())
	);
}

#[test]
fn prerelease_state_skips_unplanned_and_unknown_entries() {
	let root = std::path::PathBuf::from("/workspace");
	let package = test_package(&root, "member", "1.2.3");
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root.clone(),
		packages: vec![package],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let plan = monochange_core::ReleasePlan {
		workspace_root: root.clone(),
		decisions: vec![
			monochange_core::ReleaseDecision {
				package_id: "cargo:crates/member/Cargo.toml".to_string(),
				trigger_type: "direct-change".to_string(),
				recommended_bump: monochange_core::BumpSeverity::Patch,
				planned_version: None,
				group_id: None,
				reasons: Vec::new(),
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			monochange_core::ReleaseDecision {
				package_id: "missing".to_string(),
				trigger_type: "direct-change".to_string(),
				recommended_bump: monochange_core::BumpSeverity::Patch,
				planned_version: Some(semver::Version::parse("1.2.4-alpha.0").unwrap()),
				group_id: None,
				reasons: Vec::new(),
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
		],
		groups: vec![monochange_core::PlannedVersionGroup {
			group_id: "skipped".to_string(),
			display_name: "Skipped".to_string(),
			members: Vec::new(),
			mismatch_detected: false,
			planned_version: None,
			recommended_bump: monochange_core::BumpSeverity::Patch,
		}],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		..Default::default()
	};

	let prepared = build_prerelease_state_update(&root, &plan, &discovery, None, &config).unwrap();
	let state: serde_json::Value = serde_json::from_slice(&prepared.state_update.content).unwrap();

	assert_eq!(state["packages"].as_object().unwrap().len(), 0);
	assert_eq!(state["groups"].as_object().unwrap().len(), 0);
}

#[test]
fn fixed_prerelease_base_overrides_group_planned_version() {
	let root = std::path::PathBuf::from("/workspace");
	let mut member = test_package(&root, "member", "1.2.3");
	member.version_group_id = Some("suite".to_string());
	let discovery = monochange_core::DiscoveryReport {
		workspace_root: root,
		packages: vec![member],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};
	let mut plan = monochange_core::ReleasePlan {
		workspace_root: std::path::PathBuf::from("/workspace"),
		decisions: Vec::new(),
		groups: vec![monochange_core::PlannedVersionGroup {
			group_id: "suite".to_string(),
			display_name: "Suite".to_string(),
			members: vec!["cargo:crates/member/Cargo.toml".to_string()],
			mismatch_detected: false,
			planned_version: Some(semver::Version::parse("2.0.0").unwrap()),
			recommended_bump: monochange_core::BumpSeverity::Patch,
		}],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let config = monochange_core::PrereleaseConfiguration {
		enabled: true,
		base: monochange_core::PrereleaseBase::Fixed,
		base_version: Some(semver::Version::parse("0.5.0").unwrap()),
		..Default::default()
	};

	apply_prerelease_versions_to_plan(&mut plan, &discovery, None, &config).unwrap();

	assert_eq!(
		plan.groups[0].planned_version,
		Some(semver::Version::parse("0.5.0-alpha.0").unwrap())
	);
}
