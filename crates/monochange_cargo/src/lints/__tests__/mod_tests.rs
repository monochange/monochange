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
		options: BTreeMap::from([("fix".to_string(), json!(true))]),
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
fn manifest_repository_workspace_inherited_no_error() {
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
	assert!(results.is_empty());
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

#[test]
fn manifest_repository_missing_no_fix() {
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
	let config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fix".to_string(), serde_json::json!(false))]),
	};
	let results = rule.run(&ctx, &config);
	assert_eq!(results.len(), 1);
	assert!(results[0].fix.is_none());
}
