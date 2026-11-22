use std::path::PathBuf;

use monochange_core::MonochangeResult;
use monochange_core::lint::LintCategory;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintMaturity;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintTargetMetadata;

use super::*;

#[derive(Default)]
struct ExampleSuite;

struct ExampleRule {
	rule: LintRule,
}

impl ExampleRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"example/no-bad",
				"No bad",
				"Flags files containing the word bad",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for ExampleRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if let Some(start) = ctx.contents.find("bad") {
			vec![LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1).with_span(start, start + 3),
				"found bad",
				config.severity(),
			)]
		} else {
			Vec::new()
		}
	}
}

impl LintSuite for ExampleSuite {
	fn suite_id(&self) -> &'static str {
		"example"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![Box::new(ExampleRule::new())]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"example/recommended",
				"Example recommended",
				"Recommended example lints",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([(
				"example/no-bad".to_string(),
				LintRuleConfig::Severity(LintSeverity::Error),
			)])),
		]
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		_configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		Ok(vec![LintTarget::new(
			workspace_root.to_path_buf(),
			workspace_root.join("example.txt"),
			"first line\nthis is bad",
			LintTargetMetadata {
				ecosystem: "example".to_string(),
				relative_path: PathBuf::from("example.txt"),
				package_name: None,
				package_id: None,
				group_id: None,
				managed: false,
				private: None,
				publishable: None,
			},
			Box::new(()),
		)])
	}
}

#[derive(Default)]
struct EmptyTargetSuite;

impl LintSuite for EmptyTargetSuite {
	fn suite_id(&self) -> &'static str {
		"empty-target"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		Vec::new()
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		_configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		Ok(vec![LintTarget::new(
			workspace_root.to_path_buf(),
			workspace_root.join("empty.txt"),
			"fine",
			LintTargetMetadata {
				ecosystem: "empty-target".to_string(),
				relative_path: PathBuf::from("empty.txt"),
				package_name: None,
				package_id: None,
				group_id: None,
				managed: false,
				private: None,
				publishable: None,
			},
			Box::new(()),
		)])
	}
}

fn sample_workspace_configuration(root: &Path) -> WorkspaceConfiguration {
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
		lints: WorkspaceLintSettings::default(),
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
fn merge_config_and_selector_helpers_cover_edge_cases() {
	assert!(merge_config(None, None).is_none());
	assert!(!lint_path_pattern_matches(
		"[",
		"packages/example/package.json",
		"include"
	));
	assert_eq!(
		merge_config(None, Some(&LintRuleConfig::Severity(LintSeverity::Warning)))
			.expect("config")
			.severity(),
		LintSeverity::Warning,
	);
	assert_eq!(
		merge_config(Some(LintRuleConfig::Severity(LintSeverity::Error)), None)
			.expect("config")
			.severity(),
		LintSeverity::Error,
	);

	let target = LintTarget::new(
		".",
		"example.txt",
		"good",
		LintTargetMetadata {
			ecosystem: "example".to_string(),
			relative_path: PathBuf::from("example.txt"),
			package_name: None,
			package_id: Some("pkg".to_string()),
			group_id: Some("grp".to_string()),
			managed: false,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(()),
	);
	assert!(!selector_matches(
		&LintSelector {
			ecosystems: vec!["cargo".to_string()],
			..LintSelector::default()
		},
		&target,
	));
	assert!(!selector_matches(
		&LintSelector {
			paths: vec!["[".to_string()],
			..LintSelector::default()
		},
		&target,
	));
	assert!(!selector_matches(
		&LintSelector {
			package_ids: vec!["other".to_string()],
			..LintSelector::default()
		},
		&target,
	));
	assert!(!selector_matches(
		&LintSelector {
			group_ids: vec!["other".to_string()],
			..LintSelector::default()
		},
		&target,
	));
	assert!(!selector_matches(
		&LintSelector {
			managed: Some(true),
			..LintSelector::default()
		},
		&target,
	));
	assert!(!selector_matches(
		&LintSelector {
			private: Some(true),
			..LintSelector::default()
		},
		&target,
	));
	assert!(!selector_matches(
		&LintSelector {
			publishable: Some(false),
			..LintSelector::default()
		},
		&target,
	));
}

#[test]
fn scope_presets_and_empty_targets_are_covered() {
	let root = tempfile::tempdir().unwrap();
	let configuration = sample_workspace_configuration(root.path());
	let settings = WorkspaceLintSettings {
		scopes: vec![monochange_core::lint::LintScopeConfig {
			name: Some("preset scope".to_string()),
			selector: LintSelector {
				ecosystems: vec!["example".to_string()],
				paths: vec!["*.txt".to_string()],
				package_ids: Vec::new(),
				group_ids: Vec::new(),
				managed: None,
				private: None,
				publishable: None,
			},
			presets: vec!["example/recommended".to_string()],
			rules: BTreeMap::new(),
		}],
		..WorkspaceLintSettings::default()
	};
	let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
	let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
	assert_eq!(report.error_count, 1);

	let empty_linter = Linter::new(
		vec![Box::new(EmptyTargetSuite)],
		WorkspaceLintSettings::default(),
	);
	let empty_report =
		empty_linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
	assert!(empty_report.results.is_empty());
}
