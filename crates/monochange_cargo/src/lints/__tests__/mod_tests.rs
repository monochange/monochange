use std::fs;

use monochange_config::load_workspace_configuration;
use monochange_test_helpers::fixture_path;
use serde_json::json;

use super::*;

fn cargo_target(contents: &str, managed: bool, publishable: bool) -> LintTarget {
	cargo_target_with_repo(contents, managed, publishable, String::new())
}

fn cargo_target_with_repo(
	contents: &str,
	managed: bool,
	publishable: bool,
	repo_url: String,
) -> LintTarget {
	LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("Cargo.toml").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: Path::new("Cargo.toml").to_path_buf(),
			package_name: Some("example".to_string()),
			package_id: managed.then(|| "example".to_string()),
			group_id: None,
			managed,
			private: Some(!publishable),
			publishable: Some(publishable),
		},
		Box::new(CargoLintFile {
			document: contents.parse::<DocumentMut>().unwrap(),
			workspace_package_names: Arc::new(BTreeSet::from([
				"internal_dep".to_string(),
				"serde".to_string(),
			])),
			workspace_package_publishable: Arc::new(BTreeMap::from([
				("internal_dep".to_string(), false),
				("serde".to_string(), true),
			])),
			repo_url: Arc::new(repo_url),
			default_branch: Arc::new(String::from("main")),
		}),
	)
}

fn config() -> LintRuleConfig {
	LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::new(),
	}
}

#[test]
fn presets_are_exposed() {
	let presets = CargoLintSuite.presets();
	assert_eq!(presets.len(), 3);
	assert_eq!(
		presets.first().map(|preset| preset.id.as_str()),
		Some("cargo/baseline")
	);
	assert_eq!(
		presets.get(1).map(|preset| preset.id.as_str()),
		Some("cargo/recommended")
	);
	let baseline = presets
		.first()
		.unwrap_or_else(|| panic!("expected cargo baseline preset"));
	assert_eq!(
		baseline.rules.get("cargo/required-package-fields"),
		Some(&LintRuleConfig::Severity(LintSeverity::Warning))
	);
	assert_eq!(
		baseline.rules.get("cargo/sorted-dependencies"),
		Some(&LintRuleConfig::Severity(LintSeverity::Off))
	);
}

#[test]
fn dependency_field_order_rule_reports_and_fixes() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dependencies.serde]
features = ["derive"]
workspace = true
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = DependencyFieldOrderRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn internal_dependency_workspace_rule_reports_and_fixes() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dependencies]
internal_dep = { path = "../internal_dep", version = "0.1.0" }
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = InternalDependencyWorkspaceRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("internal dependency `internal_dep`")
	);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn publishable_dependency_rule_reports_unpublished_workspace_deps() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dev-dependencies]
internal_dep = { workspace = true }
serde = { workspace = true }
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = PublishableDependencyRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("unpublished workspace package `internal_dep`")
	);
}

#[test]
fn publishable_dependency_rule_skips_private_packages() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dev-dependencies]
internal_dep = { workspace = true }
"#,
		true,
		false,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = PublishableDependencyRule::new().run(&ctx, &config());
	assert!(results.is_empty());
}

#[test]
fn publishable_dependency_rule_skips_unparsed_targets_and_non_table_sections() {
	let target = cargo_target(
		r#"dependencies = "not a table"

[package]
name = "example"
version = "0.1.0"
"#,
		true,
		true,
	);
	let non_cargo_parsed = "not a Cargo lint file";
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: &non_cargo_parsed,
	};
	assert!(
		PublishableDependencyRule::new()
			.run(&ctx, &config())
			.is_empty()
	);

	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	assert!(
		PublishableDependencyRule::new()
			.run(&ctx, &config())
			.is_empty()
	);
}

#[test]
fn publishable_dependency_rule_metadata_is_exposed() {
	let rule = PublishableDependencyRule::new();
	assert_eq!(
		LintRuleRunner::rule(&rule).id,
		"cargo/publishable-dependencies"
	);
}

#[test]
fn required_package_fields_rule_supports_custom_fields() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"
description = "ok"
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fields".to_string(), json!(["description", "license"]))]),
	};
	let results = RequiredPackageFieldsRule::new().run(&ctx, &config);
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("license")
	);
}

#[test]
fn sorted_dependencies_rule_reports_and_fixes() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dependencies]
zzz = "1"
aaa = "1"
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = SortedDependenciesRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn unlisted_package_private_rule_reports_for_public_unmanaged_packages() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"
"#,
		false,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = UnlistedPackagePrivateRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn collect_targets_marks_configured_packages_as_managed() {
	let root = fixture_path!("monochange/release-base");
	let configuration = load_workspace_configuration(&root).unwrap();
	let targets = CargoLintSuite
		.collect_targets(&root, &configuration)
		.unwrap();
	assert!(targets.iter().any(|target| target.metadata.managed));
	assert!(
		targets
			.iter()
			.all(|target| target.metadata.ecosystem == "cargo")
	);
}

#[test]
fn manifest_repository_correct_no_error() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = \"https://github.com/foo/bar\"\n";
	let target = cargo_target(contents, true, true);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = LintRuleConfig::Severity(LintSeverity::Error);
	let results = rule.run(&ctx, &config);
	assert!(results.is_empty());
}

#[test]
fn manifest_repository_missing() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"hello\"\nversion = \"0.1.0\"\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert_eq!(results.len(), 1);
	assert_eq!(results[0].severity, LintSeverity::Error);
	assert!(results[0].fix.is_some());
}

#[test]
fn manifest_repository_wrong_value() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = \"https://wrong-url.example.com\"\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert_eq!(results.len(), 1);
}

#[test]
fn manifest_repository_empty_repo_url_skipped() {
	let rule = ManifestRepositoryRule::new();
	let contents =
		"[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = \"https://example.com\"\n";
	let target = cargo_target(contents, true, true);
	// repo_url is empty string (no source config), so rule is skipped
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert!(results.is_empty());
}

#[test]
fn manifest_repository_correct_with_repo_url() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = \"https://github.com/foo/bar\"\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert!(results.is_empty());
}

#[test]
fn manifest_repository_workspace_inherited_no_workspace_root_file() {
	let rule = ManifestRepositoryRule::new();
	let contents =
		"[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert!(
		results.is_empty(),
		"expected skip when workspace root Cargo.toml cannot be read, got {results:?}"
	);
}

#[test]
fn manifest_repository_workspace_inherited_resolves_correct_value() {
	let rule = ManifestRepositoryRule::new();
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root_manifest = "[workspace.package]\nrepository = \"https://github.com/foo/bar\"\n";
	fs::write(temp.path().join("Cargo.toml"), root_manifest)
		.unwrap_or_else(|error| panic!("write root manifest: {error}"));
	let contents =
		"[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: temp.path(),
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert!(
		results.is_empty(),
		"expected no error when inherited repository matches expected, got {results:?}"
	);
}

#[test]
fn manifest_repository_workspace_inherited_resolves_wrong_value() {
	let rule = ManifestRepositoryRule::new();
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root_manifest = "[workspace.package]\nrepository = \"https://github.com/foo/bar\"\n";
	fs::write(temp.path().join("Cargo.toml"), root_manifest)
		.unwrap_or_else(|error| panic!("write root manifest: {error}"));
	let subdir = temp.path().join("crates/hello");
	fs::create_dir_all(&subdir).unwrap_or_else(|error| panic!("create subdir: {error}"));
	let manifest_path = subdir.join("Cargo.toml");
	fs::write(
		&manifest_path,
		"[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n",
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let contents =
		fs::read_to_string(&manifest_path).unwrap_or_else(|error| panic!("read manifest: {error}"));
	let target = LintTarget::new(
		temp.path().to_path_buf(),
		manifest_path,
		contents.clone(),
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: Path::new("crates/hello/Cargo.toml").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: true,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(CargoLintFile {
			document: contents.parse::<DocumentMut>().unwrap(),
			workspace_package_names: Arc::new(BTreeSet::new()),
			workspace_package_publishable: Arc::new(BTreeMap::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new(String::from("main")),
		}),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert_eq!(results.len(), 1, "expected one error, got {results:?}");
	assert!(
		results[0]
			.message
			.contains("workspace-inherited repository resolves to"),
		"unexpected message: {}",
		results[0].message
	);
	assert!(results[0].fix.is_some(), "expected autofix to be offered");
	let fix = results[0].fix.as_ref().unwrap();
	assert_eq!(fix.edits.len(), 1);
	let replacement = &fix.edits[0].replacement;
	assert!(
		replacement.contains("https://github.com/foo/bar/tree/main/crates/hello"),
		"expected replacement to contain expected URL, got: {replacement}"
	);
	assert!(
		!replacement.contains("workspace"),
		"expected replacement to remove workspace inheritance, got: {replacement}"
	);
}

#[test]
fn manifest_repository_workspace_inherited_opt_out() {
	let rule = ManifestRepositoryRule::new();
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root_manifest = "[workspace.package]\nrepository = \"https://github.com/foo/bar\"\n";
	fs::write(temp.path().join("Cargo.toml"), root_manifest)
		.unwrap_or_else(|error| panic!("write root manifest: {error}"));
	let subdir = temp.path().join("crates/hello");
	fs::create_dir_all(&subdir).unwrap_or_else(|error| panic!("create subdir: {error}"));
	let manifest_path = subdir.join("Cargo.toml");
	fs::write(
		&manifest_path,
		"[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n",
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let contents =
		fs::read_to_string(&manifest_path).unwrap_or_else(|error| panic!("read manifest: {error}"));
	let target = LintTarget::new(
		temp.path().to_path_buf(),
		manifest_path,
		contents.clone(),
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: Path::new("crates/hello/Cargo.toml").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: true,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(CargoLintFile {
			document: contents.parse::<DocumentMut>().unwrap(),
			workspace_package_names: Arc::new(BTreeSet::new()),
			workspace_package_publishable: Arc::new(BTreeMap::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new(String::from("main")),
		}),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("allow_workspace_inheritance".to_string(), json!(true))]),
	};
	let results = rule.run(&ctx, &config);
	assert!(
		results.is_empty(),
		"expected skip when allow_workspace_inheritance=true, got {results:?}"
	);
}

#[test]
fn manifest_repository_workspace_inherited_falls_back_to_package_repository() {
	let rule = ManifestRepositoryRule::new();
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root_manifest = "[package]\nname = \"root\"\nversion = \"0.1.0\"\nrepository = \"https://github.com/foo/bar\"\n[workspace]\n";
	fs::write(temp.path().join("Cargo.toml"), root_manifest)
		.unwrap_or_else(|error| panic!("write root manifest: {error}"));
	let contents =
		"[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: temp.path(),
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert!(
		results.is_empty(),
		"expected no error when root package.repository matches expected, got {results:?}"
	);
}

#[test]
fn manifest_repository_wrong_value_with_fix() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nrepository = \"https://wrong.example.com\"\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert_eq!(results.len(), 1);
	assert!(results[0].fix.is_some());
	let fix = results[0].fix.as_ref().unwrap();
	assert_eq!(fix.edits.len(), 1);
	assert!(
		fix.edits[0]
			.replacement
			.contains("https://github.com/foo/bar")
	);
}

#[test]
fn manifest_repository_no_package_table() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[dependencies]\nserde = \"1.0\"\n";
	let target = cargo_target_with_repo(
		contents,
		true,
		true,
		"https://github.com/foo/bar".to_string(),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = config();
	let results = rule.run(&ctx, &config);
	assert!(results.is_empty());
}

// ---------------------------------------------------------------------------
// Autofix safety: no lint rule may ever replace a manifest with a fragment.
//
// Every fix these rules emit is either a targeted span edit or a whole-file
// rewrite produced by serializing a mutated copy of the parsed document
// (`LintFix::document`). These tests apply each rule's fixes through the real
// `monochange_lint` pipeline (`Linter::apply_fixes`) and assert the resulting
// manifest still parses and keeps all unrelated content.
//
// All manifests are written into temporary directories; tests must never rely
// on relative manifest paths, which would resolve against the crate's own
// sources.
// ---------------------------------------------------------------------------

use std::path::PathBuf;

use monochange_core::lint::LintReport;
use monochange_core::lint::WorkspaceLintSettings;
use monochange_lint::Linter;

/// The outcome of running a rule and applying its fixes through the pipeline.
struct FixApplication {
	/// Keep the temporary workspace alive for the assertions.
	_workspace: tempfile::TempDir,
	/// The manifest path inside the temporary workspace.
	manifest_path: PathBuf,
	/// Results reported by the rule.
	results: Vec<LintResult>,
	/// Fixed contents, absent when the pipeline applied nothing.
	fixed: Option<String>,
}

/// Run `rule` over a manifest written into a temporary workspace, apply the
/// rule's fixes through the real fix pipeline, and return the outcome.
///
/// `relative_manifest_dir` places the manifest inside the workspace (empty
/// string for the workspace root). When `root_manifest` is given it is written
/// to the workspace root first, so workspace-inheritance scenarios can resolve
/// inherited values.
fn run_rule_and_apply_fixes(
	rule: &dyn LintRuleRunner,
	contents: &str,
	config: &LintRuleConfig,
	relative_manifest_dir: &str,
	root_manifest: Option<&str>,
	managed: bool,
) -> FixApplication {
	let workspace = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	if let Some(root_manifest) = root_manifest {
		fs::write(workspace.path().join("Cargo.toml"), root_manifest)
			.unwrap_or_else(|error| panic!("write root manifest: {error}"));
	}

	let manifest_dir = if relative_manifest_dir.is_empty() {
		workspace.path().to_path_buf()
	} else {
		let dir = workspace.path().join(relative_manifest_dir);
		fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create {dir:?}: {error}"));
		dir
	};
	let manifest_path = manifest_dir.join("Cargo.toml");
	fs::write(&manifest_path, contents).unwrap_or_else(|error| panic!("write manifest: {error}"));

	let relative_manifest = if relative_manifest_dir.is_empty() {
		PathBuf::from("Cargo.toml")
	} else {
		Path::new(relative_manifest_dir).join("Cargo.toml")
	};
	let target = LintTarget::new(
		workspace.path().to_path_buf(),
		manifest_path.clone(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: relative_manifest,
			package_name: Some("example".to_string()),
			package_id: Some("example".to_string()),
			group_id: None,
			managed,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(CargoLintFile {
			document: contents
				.parse::<DocumentMut>()
				.unwrap_or_else(|error| panic!("parse manifest: {error}")),
			workspace_package_names: Arc::new(BTreeSet::from(["internal_dep".to_string()])),
			workspace_package_publishable: Arc::new(BTreeMap::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new(String::from("main")),
		}),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = rule.run(&ctx, config);

	let linter = Linter::new(
		vec![Box::new(CargoLintSuite)],
		WorkspaceLintSettings::default(),
	);
	let mut report = LintReport::new();
	for result in results.iter().cloned() {
		report.add(result);
	}
	let fixed = linter.apply_fixes(&report).get(&manifest_path).cloned();

	FixApplication {
		_workspace: workspace,
		manifest_path,
		results,
		fixed,
	}
}

/// Assert `fixed` parses as TOML and keeps every top-level table from
/// `original`, so a fix can never replace a manifest with a fragment.
fn assert_fix_preserves_document(original: &str, fixed: &str) {
	let original_doc = original
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("original manifest must parse: {error}"));
	let fixed_doc = fixed
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("fixed manifest must parse as TOML: {error}\n{fixed}"));
	let original_root = original_doc.as_table();
	let fixed_root = fixed_doc.as_table();
	for key in original_root.iter().map(|(key, _)| key) {
		assert!(
			fixed_root.get(key).is_some(),
			"top-level table `{key}` was dropped by the fix:\n--- original ---\n{original}\n--- fixed ---\n{fixed}"
		);
	}
}

#[test]
fn manifest_repository_wrong_value_fix_preserves_manifest() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\nrepository = \"https://wrong.example.com\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	// The whole manifest survives; only the repository value changes.
	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("version = \"0.1.0\""),
		"lost version: {fixed}"
	);
	assert!(
		fixed.contains("edition = \"2021\""),
		"lost edition: {fixed}"
	);
	assert!(
		fixed.contains("[dependencies]"),
		"lost dependencies section: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost serde dependency: {fixed}"
	);
	assert!(
		fixed.contains("repository = \"https://github.com/foo/bar\""),
		"repository not updated: {fixed}"
	);
	assert!(
		!fixed.contains("wrong.example.com"),
		"stale value kept: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn manifest_repository_missing_fix_preserves_manifest() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("edition = \"2021\""),
		"lost edition: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost serde dependency: {fixed}"
	);
	assert!(
		fixed.contains("repository = \"https://github.com/foo/bar\""),
		"repository not inserted: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn manifest_repository_section_form_fix_preserves_manifest() {
	// `repository` declared as a `[package.repository]` table. The rule must
	// rewrite it into a plain string without losing any other content.
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\n\n[package.repository]\nworkspace = true\n\n[dependencies]\nserde = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(
		application.results.len(),
		1,
		"expected the section form to be flagged"
	);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost serde dependency: {fixed}"
	);
	assert!(
		fixed.contains("repository = \"https://github.com/foo/bar\""),
		"repository not set to the expected URL: {fixed}"
	);
	assert!(
		!fixed.contains("workspace = true"),
		"inheritance kept: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn manifest_repository_workspace_inherited_opt_out_never_touches_files() {
	// The exact configuration from the bug report: enabling the rule with
	// `allow_workspace_inheritance = true` must not modify manifests that use
	// `repository = { workspace = true }`.
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n\n[dependencies]\nserde = \"1.0\"\n";
	let root_manifest = "[workspace.package]\nrepository = \"https://github.com/foo/bar\"\n";
	let opt_in_config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("allow_workspace_inheritance".to_string(), json!(true))]),
	};
	let application = run_rule_and_apply_fixes(
		&rule,
		contents,
		&opt_in_config,
		"crates/example",
		Some(root_manifest),
		true,
	);
	assert!(
		application.results.is_empty(),
		"opted-in workspace inheritance must never report or fix, got {:?}",
		application.results
	);
	assert!(
		application.fixed.is_none(),
		"no fixes should apply, got {:?}",
		application.fixed
	);
	assert_eq!(
		fs::read_to_string(&application.manifest_path)
			.unwrap_or_else(|error| panic!("read manifest: {error}")),
		contents,
		"the manifest must remain byte-identical"
	);
}

#[test]
fn manifest_repository_workspace_inherited_mismatch_fix_preserves_manifest() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n\n[dependencies]\nserde = \"1.0\"\n";
	let root_manifest = "[workspace.package]\nrepository = \"https://github.com/foo/bar\"\n";
	let application = run_rule_and_apply_fixes(
		&rule,
		contents,
		&config(),
		"crates/example",
		Some(root_manifest),
		true,
	);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost serde dependency: {fixed}"
	);
	assert!(
		fixed.contains("repository = \"https://github.com/foo/bar/tree/main/crates/example\""),
		"repository not set to the subdirectory URL: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn manifest_repository_workspace_inherited_matching_root_never_touches_files() {
	let rule = ManifestRepositoryRule::new();
	let contents =
		"[package]\nname = \"example\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n";
	let root_manifest = "[workspace.package]\nrepository = \"https://github.com/foo/bar\"\n";
	let application =
		run_rule_and_apply_fixes(&rule, contents, &config(), "", Some(root_manifest), true);
	assert!(application.results.is_empty());
	assert!(application.fixed.is_none());
}

#[test]
fn dependency_field_order_fix_preserves_manifest() {
	let rule = DependencyFieldOrderRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\n\n[dependencies.serde]\nfeatures = [\"derive\"]\nworkspace = true\n\n[dev-dependencies]\ntempfile = \"3.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("tempfile = \"3.0\""),
		"lost dev-dependency: {fixed}"
	);
	assert!(
		fixed.contains("workspace = true") && fixed.contains("features = [\"derive\"]"),
		"lost serde fields: {fixed}"
	);
	// workspace must now come before features inside [dependencies.serde]
	let serde_section = fixed
		.split("[dependencies.serde]")
		.nth(1)
		.unwrap_or_else(|| panic!("missing serde section: {fixed}"));
	let workspace_pos = serde_section
		.find("workspace = true")
		.expect("workspace field");
	let features_pos = serde_section.find("features").expect("features field");
	assert!(
		workspace_pos < features_pos,
		"fields not reordered: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn internal_dependency_workspace_fix_preserves_manifest() {
	let rule = InternalDependencyWorkspaceRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\n\n[dependencies]\ninternal_dep = { path = \"../internal_dep\", version = \"0.1.0\" }\nserde = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost serde dependency: {fixed}"
	);
	assert!(
		fixed.contains("internal_dep = { workspace = true }"),
		"internal dependency not rewritten: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn sorted_dependencies_fix_preserves_manifest() {
	let rule = SortedDependenciesRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0\"\naiohttp = \"1.0\"\n\n[dev-dependencies]\nzlib = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("zlib = \"1.0\""),
		"lost dev-dependency: {fixed}"
	);
	let deps_section = fixed
		.split("[dependencies]")
		.nth(1)
		.unwrap_or_else(|| panic!("missing dependencies section: {fixed}"));
	let serde_pos = deps_section.find("serde").expect("serde entry");
	let aiohttp_pos = deps_section.find("aiohttp").expect("aiohttp entry");
	assert!(
		aiohttp_pos < serde_pos,
		"dependencies not sorted alphabetically: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn unlisted_package_private_fix_preserves_manifest() {
	let rule = UnlistedPackagePrivateRule::new();
	let contents =
		"[package]\nname = \"example\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, false);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost dependency: {fixed}"
	);
	assert!(
		fixed.contains("publish = false"),
		"publish not inserted: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn non_autofixable_cargo_rules_never_emit_fixes() {
	// `cargo/required-package-fields` and `cargo/publishable-dependencies` are
	// declared non-autofixable; they must never attach a fix to their results.
	let required = RequiredPackageFieldsRule::new();
	let publishable = PublishableDependencyRule::new();
	let contents = "[package]\nname = \"example\"\n\n[dependencies]\ninternal_dep = \"0.1.0\"\n";
	let application = run_rule_and_apply_fixes(&required, contents, &config(), "", None, true);
	for result in &application.results {
		assert!(
			result.fix.is_none(),
			"required-package-fields emitted a fix"
		);
	}
	let application = run_rule_and_apply_fixes(&publishable, contents, &config(), "", None, true);
	for result in &application.results {
		assert!(
			result.fix.is_none(),
			"publishable-dependencies emitted a fix"
		);
	}
}

#[test]
fn fix_loop_converges_without_losing_manifest_content() {
	// End-to-end: run the entire cargo suite over a workspace whose manifests
	// violate several rules at once, then apply fixes iteratively (like
	// `monochange check --fix`) until no fixable results remain. Every
	// intermediate manifest must keep parsing and every original field must
	// survive.
	let workspace = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = workspace.path();

	fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
package_type = "cargo"
changelog = false

[source]
provider = "github"
owner = "acme"
repo = "widgets"

[lints.rules]
"cargo/dependency-field-order" = "error"
"cargo/internal-dependency-workspace" = "error"
"cargo/required-package-fields" = "off"
"cargo/sorted-dependencies" = "error"
"cargo/unlisted-package-private" = { level = "error", fix = true }
"cargo/manifest-repository" = "error"

[ecosystems.cargo]
enabled = true

[package.core]
path = "crates/core"
type = "cargo"
version = "0.1.0"

[package.utils]
path = "crates/utils"
type = "cargo"
version = "0.1.0"
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nrepository = \"https://github.com/acme/widgets\"\n",
	)
	.unwrap_or_else(|error| panic!("write root manifest: {error}"));

	let core_dir = root.join("crates/core");
	fs::create_dir_all(&core_dir).unwrap_or_else(|error| panic!("create core dir: {error}"));
	let core_manifest = "[package]\nname = \"core\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n\n[dependencies]\nutils = \"0.1.0\"\nserde = \"1.0\"\n\n[dev-dependencies.serde]\nfeatures = [\"derive\"]\nworkspace = true\n";
	fs::write(core_dir.join("Cargo.toml"), core_manifest)
		.unwrap_or_else(|error| panic!("write core manifest: {error}"));

	let utils_dir = root.join("crates/utils");
	fs::create_dir_all(&utils_dir).unwrap_or_else(|error| panic!("create utils dir: {error}"));
	let utils_manifest = "[package]\nname = \"utils\"\nversion = \"0.1.0\"\n\n[dependencies]\nzlib = \"1.0\"\nserde = \"1.0\"\n";
	fs::write(utils_dir.join("Cargo.toml"), utils_manifest)
		.unwrap_or_else(|error| panic!("write utils manifest: {error}"));

	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load configuration: {error}"));
	let linter = Linter::new(
		vec![Box::new(CargoLintSuite)],
		WorkspaceLintSettings::default(),
	);

	for _iteration in 0..10 {
		let report = linter.lint_workspace(
			root,
			&configuration,
			&monochange_core::lint::NoopLintProgressReporter,
		);
		if report.autofixable().is_empty() {
			break;
		}

		for (path, fixed) in linter.apply_fixes(&report) {
			// Every intermediate write must produce a parseable manifest.
			fixed.parse::<DocumentMut>().unwrap_or_else(|error| {
				panic!(
					"fix loop produced unparseable {}: {error}\n{fixed}",
					path.display()
				)
			});
			fs::write(&path, fixed)
				.unwrap_or_else(|error| panic!("write fixed manifest {}: {error}", path.display()));
		}
	}

	let report = linter.lint_workspace(
		root,
		&configuration,
		&monochange_core::lint::NoopLintProgressReporter,
	);
	assert_eq!(
		report.autofixable().len(),
		0,
		"fix loop did not converge; remaining: {:?}",
		report
			.autofixable()
			.iter()
			.map(|result| format!("{}: {}", result.rule_id, result.message))
			.collect::<Vec<_>>()
	);

	let fixed_core = fs::read_to_string(core_dir.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read core manifest: {error}"));
	assert!(
		fixed_core.contains("name = \"core\""),
		"core package name lost: {fixed_core}"
	);
	assert!(
		fixed_core.contains("serde = \"1.0\""),
		"core serde lost: {fixed_core}"
	);
	assert!(
		fixed_core.contains("utils = { workspace = true }"),
		"core internal dependency not rewritten: {fixed_core}"
	);
	assert!(
		fixed_core
			.contains("repository = \"https://github.com/acme/widgets/tree/main/crates/core\""),
		"core repository not fixed: {fixed_core}"
	);
	let core_dev_section = fixed_core
		.split("[dev-dependencies.serde]")
		.nth(1)
		.unwrap_or_else(|| panic!("missing core dev-dependencies.serde section: {fixed_core}"));
	let core_workspace_pos = core_dev_section
		.find("workspace = true")
		.unwrap_or_else(|| panic!("core workspace field lost: {fixed_core}"));
	let core_features_pos = core_dev_section
		.find("features")
		.unwrap_or_else(|| panic!("core features field lost: {fixed_core}"));
	assert!(
		core_workspace_pos < core_features_pos,
		"core fields not reordered: {fixed_core}"
	);
	assert_fix_preserves_document(core_manifest, &fixed_core);

	let fixed_utils = fs::read_to_string(utils_dir.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read utils manifest: {error}"));
	assert!(
		fixed_utils.contains("name = \"utils\""),
		"utils package name lost: {fixed_utils}"
	);
	assert!(
		fixed_utils.contains("zlib = \"1.0\""),
		"utils zlib lost: {fixed_utils}"
	);
	assert!(
		fixed_utils.contains("serde = \"1.0\""),
		"utils serde lost: {fixed_utils}"
	);
	assert!(
		fixed_utils
			.contains("repository = \"https://github.com/acme/widgets/tree/main/crates/utils\""),
		"utils repository not fixed: {fixed_utils}"
	);
	assert_fix_preserves_document(utils_manifest, &fixed_utils);
}
