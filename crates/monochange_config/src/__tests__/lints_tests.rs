use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::VersionSource;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintTargetMetadata;
use serde_json::json;

use super::*;

fn change(package: &str, bump: Option<BumpSeverity>, change_type: Option<&str>) -> RawChangeEntry {
	RawChangeEntry {
		package: package.to_string(),
		bump,
		version: None,
		reason: None,
		details: None,
		change_type: change_type.map(ToString::to_string),
		caused_by: Vec::new(),
	}
}

fn lint_file(body: &str, changes: Vec<RawChangeEntry>) -> ChangesetLintFile {
	ChangesetLintFile {
		body: body.to_string(),
		changes,
		raw_values: Vec::new(),
		target_types: BTreeMap::new(),
	}
}

/// Build target change-type metadata from `(type, default bump)` pairs.
fn target_types(types: &[(&str, BumpSeverity)]) -> BTreeMap<String, TargetChangeTypes> {
	let mut target_types = BTreeMap::new();
	target_types.insert(
		"core".to_string(),
		TargetChangeTypes {
			default_bumps: types
				.iter()
				.map(|(name, bump)| ((*name).to_string(), *bump))
				.collect(),
		},
	);
	target_types
}

fn metadata() -> LintTargetMetadata {
	LintTargetMetadata {
		ecosystem: "changesets".to_string(),
		relative_path: PathBuf::from(".changeset/change.md"),
		package_name: None,
		package_id: None,
		group_id: None,
		managed: false,
		private: None,
		publishable: None,
	}
}

fn workspace_configuration(root: &Path) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
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

fn severity(severity: LintSeverity) -> LintRuleConfig {
	LintRuleConfig::Severity(severity)
}

fn detailed(options: BTreeMap<String, serde_json::Value>) -> LintRuleConfig {
	LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options,
	}
}

fn must<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
	result.unwrap_or_else(|error| panic!("{context}: {error}"))
}

fn run_rule<R>(rule: &R, file: &ChangesetLintFile, config: &LintRuleConfig) -> Vec<LintResult>
where
	R: LintRuleRunner,
{
	let metadata = metadata();
	let manifest_path = Path::new(".changeset/change.md");
	let ctx = LintContext {
		workspace_root: Path::new("."),
		manifest_path,
		contents: &file.body,
		metadata: &metadata,
		parsed: file,
	};
	rule.run(&ctx, config)
}

#[test]
fn summary_rule_requires_first_body_line_to_be_heading() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("required".to_string(), json!(true));
	let config = detailed(options);

	let empty_results = run_rule(&rule, &lint_file("", Vec::new()), &config);
	assert!(
		empty_results
			.iter()
			.any(|result| { result.message == "changeset body must start with a summary heading" })
	);

	let paragraph_results = run_rule(&rule, &lint_file("summary paragraph", Vec::new()), &config);
	assert!(
		paragraph_results
			.iter()
			.any(|result| { result.message == "changeset body must start with a summary heading" })
	);
}

#[test]
fn summary_rule_reports_length_period_and_prefix_issues_together() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("min_length".to_string(), json!(30));
	options.insert("max_length".to_string(), json!(5));
	options.insert("forbid_trailing_period".to_string(), json!(true));
	options.insert("forbid_conventional_commit_prefix".to_string(), json!(true));
	let results = run_rule(
		&rule,
		&lint_file("# feat: add.", Vec::new()),
		&detailed(options),
	);

	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset summary must be at least 30 characters" })
	);
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset header must be at most 5 characters" })
	);
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset summary must not end with a period" })
	);
	assert!(results.iter().any(|result| {
		result.message == "changeset summary must not use a conventional-commit prefix"
	}));
}

#[test]
fn summary_rule_require_description_fails_when_next_line_is_heading() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("require_description".to_string(), json!(true));
	let body = "# Summary\n\n## Details";
	let results = run_rule(&rule, &lint_file(body, Vec::new()), &detailed(options));

	assert!(results.iter().any(|result| {
		result.message == "changeset summary must be followed by a description paragraph"
	}));
}

#[test]
fn summary_rule_reports_heading_level_and_length_issues_together() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("required".to_string(), json!(true));
	options.insert("heading_level".to_string(), json!(1));
	options.insert("max_heading_length".to_string(), json!(5));
	let results = run_rule(
		&rule,
		&lint_file("## This heading is way too long", Vec::new()),
		&detailed(options),
	);

	// Should report both: wrong heading level AND heading too long.
	// Previously only the heading level error was reported due to early return.
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset summary heading must use level 1, found level 2")
	}));
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset header must be at most 5 characters")
	}));
}

#[test]
fn type_scope_rule_reports_all_constraints_for_matching_changes() {
	let rule = TypeScopeRule::new("app_feature");
	let file = lint_file(
		"# Summary\n\n## Forbidden\n\nA body without the requested section.",
		vec![
			change("core", Some(BumpSeverity::Patch), Some("app_feature")),
			change("cli", Some(BumpSeverity::Patch), Some("fix")),
		],
	);
	let mut options = BTreeMap::new();
	options.insert("required_bump".to_string(), json!("minor"));
	options.insert("required_sections".to_string(), json!(["User impact", 7]));
	options.insert("forbidden_headings".to_string(), json!(["Forbidden"]));
	options.insert("min_body_chars".to_string(), json!(200));
	options.insert("max_body_chars".to_string(), json!(10));

	options.insert("require_code_block".to_string(), json!(true));

	let results = run_rule(&rule, &file, &detailed(options));
	assert!(
		results
			.iter()
			.all(|result| { result.rule_id == "changesets/types/app_feature" })
	);
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset type `app_feature` requires bump `minor`, found `patch`")
	}));
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset must include a `User impact` section" })
	);
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset must not use `Forbidden` as a heading" })
	);
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset body must be at least 200 characters" })
	);
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset body must be at most 10 characters" })
	);
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset must include a fenced code block" })
	);
}

#[test]
fn lint_rule_config_extension_reads_bool_and_string_list_options() {
	let mut options = BTreeMap::new();
	options.insert("enabled".to_string(), json!(true));
	options.insert("names".to_string(), json!(["one", 2, "two"]));
	let config = detailed(options);

	assert!(<LintRuleConfig as LintRuleConfigExt>::bool_option(
		&config, "enabled", false
	));
	assert!(<LintRuleConfig as LintRuleConfigExt>::bool_option(
		&config, "missing", true
	));
	let names = <LintRuleConfig as LintRuleConfigExt>::string_list_option(&config, "names")
		.expect("string list option");
	assert_eq!(names, vec!["one".to_string(), "two".to_string()]);
	assert!(
		<LintRuleConfig as LintRuleConfigExt>::string_list_option(&config, "missing").is_none()
	);
}

// ── Prefer inline rule ─────────────────────────────────────────────────────
/// Run the prefer-inline rule against a full changeset file.
fn run_prefer_inline(
	contents: &str,
	target_types: BTreeMap<String, TargetChangeTypes>,
	config: &LintRuleConfig,
) -> Vec<LintResult> {
	let rule = PreferInlineRule::new();
	let metadata = metadata();
	let mut file = parse_changeset_for_lint(contents).expect("changeset should parse");
	file.target_types = target_types;
	let ctx = LintContext {
		workspace_root: Path::new("."),
		manifest_path: Path::new(".changeset/change.md"),
		contents,
		metadata: &metadata,
		parsed: &file,
	};
	rule.run(&ctx, config)
}

/// Apply the fix of the single reported result and return the fixed contents.
fn apply_single_fix(contents: &str, results: &[LintResult]) -> String {
	let result = results.first().expect("expected one lint result");
	let fix = result.fix.as_ref().expect("expected a fix");
	assert_eq!(fix.edits.len(), 1);
	let edit = &fix.edits[0];
	let mut fixed = contents.to_string();
	fixed.replace_range(edit.span.0..edit.span.1, &edit.replacement);
	fixed
}

#[test]
fn collect_targets_populates_target_types_from_configuration() {
	let tempdir = must(tempfile::tempdir(), "tempdir");
	let changeset_dir = tempdir.path().join(".changeset");
	must(fs::create_dir_all(&changeset_dir), "changeset dir");
	must(
		fs::write(
			changeset_dir.join("change.md"),
			"---\ncore:\n  bump: minor\n  type: feat\n---\n\n#### Add target\n",
		),
		"write changeset",
	);

	let mut configuration = workspace_configuration(tempdir.path());
	configuration
		.packages
		.push(monochange_core::PackageDefinition {
			id: "core".to_string(),
			path: tempdir.path().join("crates/core"),
			package_type: monochange_core::PackageType::Cargo,
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
			version_format: monochange_core::VersionFormat::Namespaced,
			version_source: VersionSource::default(),
			initial_version: None,
			bump_ceiling: None,
			classification_enforced: None,

			floating_tags: Vec::new(),
			publish: monochange_core::PublishSettings::default(),
			cli: None,
			display_version: None,
			values: BTreeMap::new(),
		});
	configuration.groups.push(monochange_core::GroupDefinition {
		id: "group".to_string(),
		packages: vec!["core".to_string()],
		package_max_bumps: BTreeMap::new(),
		bump_propagation: None,
		changelog: None,
		changelog_include: monochange_core::GroupChangelogInclude::default(),
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Namespaced,
		version_source: VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
	});

	let targets = must(
		ChangesetLintSuite::new().collect_targets(tempdir.path(), &configuration),
		"collect targets",
	);
	assert_eq!(targets.len(), 1);
	let parsed = targets
		.first()
		.expect("target")
		.parsed
		.downcast_ref::<ChangesetLintFile>()
		.expect("changeset lint file");
	let target = parsed
		.target_types
		.get("core")
		.expect("configured package target types");
	assert_eq!(target.default_bumps.get("feat"), Some(&BumpSeverity::Minor));
	let group = parsed
		.target_types
		.get("group")
		.expect("configured group target types");
	assert_eq!(group.default_bumps.get("feat"), Some(&BumpSeverity::Minor));
}

// The following tests exercise `frontmatter_entry_spans` directly so the raw
// scanner behavior is covered independently of the serde frontmatter parse.
fn spans_of(contents: &str) -> Option<Vec<(String, (usize, usize))>> {
	frontmatter_entry_spans(contents).map(|entries| {
		entries
			.into_iter()
			.map(|entry| (entry.package, entry.span))
			.collect()
	})
}

#[test]
fn prefer_inline_rule_handles_escaped_quoted_keys() {
	// Double-quoted keys support escapes; single-quoted keys support ''.
	let contents = "---\n\"core\\\"x\": {type: feat}\n'it''s': {type: feat}\n---\n\n#### Summary\n";
	let mut target_types = BTreeMap::new();
	target_types.insert(
		"core\"x".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("feat".to_string(), BumpSeverity::Minor)]),
		},
	);
	target_types.insert(
		"it's".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("feat".to_string(), BumpSeverity::Minor)]),
		},
	);
	let results = run_prefer_inline(contents, target_types, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 2, "unexpected results: {results:?}");

	let spans = spans_of(contents).expect("entries");
	assert_eq!(spans.len(), 2);
	assert_eq!(spans[0].0, "core\"x");
	assert_eq!(spans[1].0, "it's");
}

#[test]
fn prefer_inline_rule_rewrites_flow_entries_with_quoted_values() {
	let mut target_types = target_types(&[]);
	target_types.insert(
		"core".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([
				("feat".to_string(), BumpSeverity::Minor),
				("fix".to_string(), BumpSeverity::Patch),
				("a\"b".to_string(), BumpSeverity::Minor),
				("it's".to_string(), BumpSeverity::Minor),
			]),
		},
	);

	// Double-quoted values, including escaped quotes.
	let escaped = "---\ncore: {type: \"a\\\"b\"}\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		escaped,
		target_types.clone(),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(escaped, &results),
		"---\ncore: \"a\\\"b\"\n---\n\n#### Summary\n"
	);

	// Single-quoted values close normally.
	let single = "---\ncore: {type: 'fix'}\n---\n\n#### Summary\n";
	let results = run_prefer_inline(single, target_types.clone(), &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(single, &results),
		"---\ncore: fix\n---\n\n#### Summary\n"
	);

	// Single-quoted values support the '' escape.
	let doubled = "---\ncore: {type: 'it''s'}\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		doubled,
		target_types.clone(),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(doubled, &results),
		"---\ncore: \"it's\"\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_reports_bare_bump_entries_that_are_types() {
	// `minor` is a configured type whose default bump is Minor, so the bare
	// bump entry converts inline and gains the type.
	let contents = "---\ncore:\n  bump: minor\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[
			("feat", BumpSeverity::Minor),
			("minor", BumpSeverity::Minor),
		]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert!(
		results
			.first()
			.expect("result")
			.message
			.contains("declares a bare bump that is also a change type")
	);
	assert!(
		results
			.first()
			.expect("result")
			.message
			.contains("use the inline form `core: minor`")
	);
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: minor\n---\n\n#### Summary\n"
	);

	// Flow form works the same way.
	let flow = "---\ncore: {bump: patch}\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		flow,
		target_types(&[("patch", BumpSeverity::Patch)]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(flow, &results),
		"---\ncore: patch\n---\n\n#### Summary\n"
	);
}
