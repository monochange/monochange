use monochange_config::load_workspace_configuration;
use monochange_test_helpers::fixture_path;
use serde_json::json;

use super::*;

fn npm_target(contents: &str, managed: bool, private: bool) -> LintTarget {
	LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("./package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("package.json").to_path_buf(),
			package_name: Some("example".to_string()),
			package_id: managed.then(|| "example".to_string()),
			group_id: None,
			managed,
			private: Some(private),
			publishable: Some(!private),
		},
		Box::new(NpmLintFile {
			manifest: serde_json::from_str(contents).unwrap(),
			workspace_package_names: Arc::new(BTreeSet::from([
				"@scope/internal".to_string(),
				"shared".to_string(),
			])),
			repo_url: Arc::new(String::new()),
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
	let presets = NpmLintSuite.presets();
	assert_eq!(presets.len(), 3);
	assert_eq!(
		presets.first().map(|preset| preset.id.as_str()),
		Some("npm/baseline")
	);
	assert_eq!(
		presets.get(1).map(|preset| preset.id.as_str()),
		Some("npm/recommended")
	);
	let baseline = presets
		.first()
		.unwrap_or_else(|| panic!("expected npm baseline preset"));
	assert_eq!(
		baseline.rules.get("npm/required-package-fields"),
		Some(&LintRuleConfig::Severity(LintSeverity::Warning))
	);
	let recommended = presets
		.get(1)
		.unwrap_or_else(|| panic!("expected npm recommended preset"));
	assert_eq!(
		recommended.rules.get("npm/workspace-protocol"),
		Some(&LintRuleConfig::Severity(LintSeverity::Off))
	);
	let strict = presets
		.get(2)
		.unwrap_or_else(|| panic!("expected npm strict preset"));
	assert_eq!(
		strict.rules.get("npm/workspace-protocol"),
		Some(&LintRuleConfig::Severity(LintSeverity::Error))
	);
}

#[test]
fn workspace_protocol_rule_reports_internal_ranges() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "@scope/internal": "^1.0.0"
  }
}"#,
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
	let fallback_location = location_for_needle(&ctx, "missing-dependency");
	assert_eq!((fallback_location.line, fallback_location.column), (1, 1));
	assert_eq!(line_column_for_offset("éclair", 1), None);

	let results = WorkspaceProtocolRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn sorted_dependencies_rule_reports_unsorted_sections() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "zzz": "1",
    "aaa": "1"
  }
}"#,
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
	let results = SortedDependenciesRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
}

#[test]
fn sorted_dependencies_fix_preserves_top_level_package_json_order() {
	let contents = r#"{
  "name": "example",
  "version": "0.0.0",
  "type": "module",
  "description": "keep top-level order",
  "dependencies": {
    "zeta": "1.0.0",
    "alpha": "1.0.0"
  },
  "devDependencies": {
    "vitest": "1.0.0"
  }
}"#;
	let target = npm_target(contents, true, false);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = SortedDependenciesRule::new().run(&ctx, &config());
	let fix = results
		.first()
		.and_then(|result| result.fix.as_ref())
		.unwrap_or_else(|| panic!("expected sorted dependency fix"));
	let edit = fix
		.edits
		.first()
		.unwrap_or_else(|| panic!("expected sorted dependency edit"));
	assert_ne!(edit.span, (0, contents.len()));

	let mut fixed = contents.to_string();
	fixed.replace_range(edit.span.0..edit.span.1, &edit.replacement);
	assert!(fixed.contains(
		r#"  "dependencies": {
    "alpha": "1.0.0",
    "zeta": "1.0.0"
  },"#
	));
	assert!(fixed.contains(
		r#"  "name": "example",
  "version": "0.0.0",
  "type": "module",
  "description": "keep top-level order",
  "dependencies""#
	));
}

#[test]
fn dependency_fix_helpers_cover_minimal_and_missing_sections() {
	let contents = r#"{"dependencies":{"zeta":"1","alpha":"2"}}"#;
	assert_eq!(
		dependency_value_span(contents, "dependencies", "zeta", "1"),
		Some((24, 27))
	);
	assert_eq!(dependency_section_object_span(contents, "missing"), None);
	assert_eq!(
		dependency_value_span(contents, "dependencies", "missing", "1"),
		None
	);

	let empty = Map::new();
	let replacement =
		sorted_dependency_section_text(r#"{"dependencies":{}}"#, "dependencies", &empty, &[])
			.unwrap_or_else(|| panic!("expected replacement for empty dependency section"));
	assert_eq!(replacement, "{\n}");
	assert_eq!(
		dependency_section_object_span(r#"{"dependencies":{"alpha":"1""#, "dependencies"),
		None
	);
}

#[test]
fn required_package_fields_rule_supports_custom_fields() {
	let target = npm_target(r#"{"name":"example","description":"ok"}"#, true, false);
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
fn root_no_prod_deps_rule_moves_dependencies() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "left-pad": "1"
  }
}"#,
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
	let results = RootNoProdDepsRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn no_duplicate_dependencies_rule_prefers_dev_dependencies() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "shared": "1"
  },
  "devDependencies": {
    "shared": "1"
  }
}"#,
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
	let results = NoDuplicateDependenciesRule::new().run(&ctx, &config());
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
	let target = npm_target(r#"{"name":"example"}"#, false, false);
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
	let root = fixture_path!("cli-output/discover-mixed");
	let configuration = load_workspace_configuration(&root).unwrap();
	let targets = NpmLintSuite.collect_targets(&root, &configuration).unwrap();
	assert!(
		targets
			.iter()
			.all(|target| target.metadata.ecosystem == "npm")
	);
	assert!(targets.iter().any(|target| target.metadata.managed));
}

#[test]
fn manifest_repository_missing() {
	let rule = ManifestRepositoryRule::new();
	let contents = "{\"name\": \"hello\", \"version\": \"0.1.0\"}";
	let manifest: Value = serde_json::from_str(contents).unwrap();
	let target = LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("package.json").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: false,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(NpmLintFile {
			manifest,
			workspace_package_names: Arc::new(BTreeSet::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new("main".to_string()),
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
	assert_eq!(results.len(), 1);
}

#[test]
fn manifest_repository_empty_repo_url_skipped() {
	let rule = ManifestRepositoryRule::new();
	let contents =
		"{\"name\": \"hello\", \"version\": \"0.1.0\", \"repository\": \"https://example.com\"}";
	let manifest: Value = serde_json::from_str(contents).unwrap();
	let target = LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("package.json").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: false,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(NpmLintFile {
			manifest,
			workspace_package_names: Arc::new(BTreeSet::new()),
			repo_url: Arc::new(String::new()),
			default_branch: Arc::new("main".to_string()),
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
	assert!(results.is_empty());
}

#[test]
fn manifest_repository_wrong_value() {
	let rule = ManifestRepositoryRule::new();
	let contents = "{\"name\": \"hello\", \"version\": \"0.1.0\", \"repository\": \"https://wrong.example.com\"}";
	let manifest: Value = serde_json::from_str(contents).unwrap();
	let target = LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("package.json").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: false,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(NpmLintFile {
			manifest,
			workspace_package_names: Arc::new(BTreeSet::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new("main".to_string()),
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
	assert_eq!(results.len(), 1);
}

#[test]
fn manifest_repository_subdirectory_path() {
	let rule = ManifestRepositoryRule::new();
	let contents = "{\"name\": \"hello\", \"version\": \"0.1.0\"}";
	let manifest: Value = serde_json::from_str(contents).unwrap();
	let target = LintTarget::new(
		Path::new("/ws").to_path_buf(),
		Path::new("/ws/packages/pkg/package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("packages/pkg/package.json").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: false,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(NpmLintFile {
			manifest,
			workspace_package_names: Arc::new(BTreeSet::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new("main".to_string()),
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
	assert_eq!(results.len(), 1);
}

#[test]
fn manifest_repository_correct_value() {
	let rule = ManifestRepositoryRule::new();
	let contents = "{\"name\": \"hello\", \"version\": \"0.1.0\", \"repository\": \"https://github.com/foo/bar\"}";
	let manifest: Value = serde_json::from_str(contents).unwrap();
	let target = LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("package.json").to_path_buf(),
			package_name: Some("hello".to_string()),
			package_id: Some("hello".to_string()),
			group_id: None,
			managed: false,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(NpmLintFile {
			manifest,
			workspace_package_names: Arc::new(BTreeSet::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new("main".to_string()),
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
	assert!(results.is_empty());
}
