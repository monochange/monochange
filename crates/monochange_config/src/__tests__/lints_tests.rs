use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

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

fn run_rule_with_wrong_parsed<R>(rule: &R, config: &LintRuleConfig) -> Vec<LintResult>
where
	R: LintRuleRunner,
{
	let metadata = metadata();
	let parsed = 42_u8;
	let ctx = LintContext {
		workspace_root: Path::new("."),
		manifest_path: Path::new(".changeset/change.md"),
		contents: "# Summary",
		metadata: &metadata,
		parsed: &parsed,
	};
	rule.run(&ctx, config)
}

#[test]
fn parse_changeset_for_lint_parses_frontmatter_shapes() {
	let parsed = parse_changeset_for_lint(
		"---\r\ncore: patch\r\ncli: feature\r\napi:\r\n  bump: minor\r\n  type: migration\r\nempty: ''\r\n---\r\n\r\n# Ship changes\r\n\r\nBody\r\n",
	)
	.expect("expected changeset to parse");
	assert_eq!(parsed.body, "# Ship changes\n\nBody");
	assert_eq!(parsed.changes.len(), 4);
	assert!(
		parsed
			.changes
			.iter()
			.any(|entry| { entry.package == "core" && entry.bump == Some(BumpSeverity::Patch) })
	);
	assert!(parsed.changes.iter().any(|entry| {
		entry.package == "cli" && entry.change_type.as_deref() == Some("feature")
	}));
	assert!(parsed.changes.iter().any(|entry| {
		entry.package == "api"
			&& entry.bump == Some(BumpSeverity::Minor)
			&& entry.change_type.as_deref() == Some("migration")
	}));
	assert!(parsed.changes.iter().any(|entry| {
		entry.package == "empty" && entry.bump.is_none() && entry.change_type.is_none()
	}));
	assert_eq!(
		parsed
			.raw_values
			.iter()
			.map(|(package, _)| package.as_str())
			.collect::<Vec<_>>(),
		vec!["core", "cli", "api", "empty"]
	);
}

#[test]
fn parse_changeset_for_lint_rejects_non_changesets() {
	assert!(parse_changeset_for_lint("# No frontmatter").is_none());
	assert!(parse_changeset_for_lint("---\nnot: [valid\n---\n# Broken").is_none());
	assert!(parse_changeset_for_lint("---\n[not-a-map]\n---\n# Broken").is_none());
	assert!(parse_changeset_for_lint("---\n123: patch\n---\n# Broken").is_none());
}

#[test]
fn lint_suite_exposes_changeset_rules_and_presets() {
	let suite = ChangesetLintSuite::new();
	assert_eq!(suite.suite_id(), "changesets");

	let ids = suite
		.rules()
		.into_iter()
		.map(|rule| rule.rule().id.clone())
		.collect::<Vec<_>>();
	assert!(ids.iter().any(|id| id == "changesets/summary"));
	assert!(ids.iter().any(|id| id == "changesets/no_section_headings"));
	assert!(ids.iter().any(|id| id == "changesets/prefer-inline"));
	assert!(ids.iter().any(|id| id == "changesets/bump/none"));
	assert!(ids.iter().any(|id| id == "changesets/bump/patch"));
	assert!(ids.iter().any(|id| id == "changesets/bump/minor"));
	assert!(ids.iter().any(|id| id == "changesets/bump/major"));

	let presets = suite.presets();
	assert!(presets.iter().any(|preset| {
		preset.id == "changesets/recommended"
			&& preset.rules.contains_key("changesets/summary")
			&& preset.rules.contains_key("changesets/prefer-inline")
	}));
}

#[test]
fn collect_targets_filters_and_parses_changeset_files() {
	let tempdir = must(tempfile::tempdir(), "tempdir");
	let changeset_dir = tempdir.path().join(".changeset");
	must(fs::create_dir_all(&changeset_dir), "changeset dir");
	must(
		fs::write(
			changeset_dir.join("change.md"),
			"---\ncore: patch\n---\n\n# Add target\n",
		),
		"write changeset",
	);
	must(
		fs::write(changeset_dir.join("README.md"), "# Readme"),
		"write readme",
	);
	must(
		fs::write(changeset_dir.join("ignored"), "ignored"),
		"write no extension",
	);
	must(
		fs::write(changeset_dir.join("ignored.txt"), "ignored"),
		"write txt",
	);
	must(
		fs::write(changeset_dir.join("not-a-change.md"), "# No frontmatter"),
		"write markdown",
	);

	let configuration = workspace_configuration(tempdir.path());
	let targets = must(
		ChangesetLintSuite::new().collect_targets(tempdir.path(), &configuration),
		"collect targets",
	);
	assert_eq!(targets.len(), 1);
	let target = targets.first().expect("target");
	assert_eq!(target.metadata.ecosystem, "changesets");
	assert_eq!(
		target.metadata.relative_path,
		PathBuf::from(".changeset/change.md")
	);
	let parsed = target
		.parsed
		.downcast_ref::<ChangesetLintFile>()
		.expect("changeset lint file");
	assert_eq!(parsed.body, "# Add target");
	assert!(
		parsed
			.changes
			.iter()
			.any(|entry| { entry.package == "core" && entry.bump == Some(BumpSeverity::Patch) })
	);
}

#[test]
fn collect_targets_handles_missing_and_invalid_changeset_directories() {
	let tempdir = must(tempfile::tempdir(), "tempdir");
	let configuration = workspace_configuration(tempdir.path());
	let targets = must(
		ChangesetLintSuite::new().collect_targets(tempdir.path(), &configuration),
		"missing changeset dir is fine",
	);
	assert!(targets.is_empty());

	must(
		fs::write(tempdir.path().join(".changeset"), "not a directory"),
		"write file",
	);
	let error = ChangesetLintSuite::new()
		.collect_targets(tempdir.path(), &configuration)
		.expect_err("file changeset path should fail read_dir");
	assert!(
		error
			.to_string()
			.contains("failed to read changeset directory")
	);

	let unreadable_tempdir = must(tempfile::tempdir(), "tempdir");
	let unreadable_changeset_dir = unreadable_tempdir.path().join(".changeset");
	must(
		fs::create_dir_all(unreadable_changeset_dir.join("directory.md")),
		"directory with markdown extension",
	);
	let configuration = workspace_configuration(unreadable_tempdir.path());
	let error = ChangesetLintSuite::new()
		.collect_targets(unreadable_tempdir.path(), &configuration)
		.expect_err("directory changeset path should fail read_to_string");
	assert!(error.to_string().contains("failed to read changeset file"));
}

#[test]
fn summary_rule_respects_disabled_and_wrong_target_types() {
	let rule = SummaryRule::new();
	let file = lint_file("", Vec::new());
	let mut options = BTreeMap::new();
	options.insert("required".to_string(), json!(true));
	assert!(run_rule(&rule, &file, &severity(LintSeverity::Off)).is_empty());
	assert!(run_rule_with_wrong_parsed(&rule, &detailed(options)).is_empty());
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
fn summary_rule_enforces_heading_level_one_by_configuration() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("required".to_string(), json!(true));
	options.insert("heading_level".to_string(), json!(1));
	let results = run_rule(
		&rule,
		&lint_file("#### Too deep", Vec::new()),
		&detailed(options),
	);
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset summary heading must use level 1, found level 4")
	}));
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
fn summary_rule_enforces_default_max_heading_length_of_60() {
	let rule = SummaryRule::new();
	let long_heading = format!("# {}", "a".repeat(61));
	let results = run_rule(
		&rule,
		&lint_file(&long_heading, Vec::new()),
		&severity(LintSeverity::Error),
	);

	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset header must be at most 60 characters" }),
		"expected default max_heading_length of 60 for headings, got: {results:?}"
	);
}

#[test]
fn summary_rule_allows_long_non_heading_summary_by_default() {
	let rule = SummaryRule::new();
	let long_paragraph = "a".repeat(61);
	let results = run_rule(
		&rule,
		&lint_file(&long_paragraph, Vec::new()),
		&severity(LintSeverity::Error),
	);

	assert!(
		results.is_empty(),
		"expected no max_length violations for non-heading summary by default, got: {results:?}"
	);
}

#[test]
fn summary_rule_enforces_explicit_max_length() {
	let rule = SummaryRule::new();
	let heading = format!("# {}", "a".repeat(61));
	let mut options = BTreeMap::new();
	options.insert("max_length".to_string(), json!(60));
	let config = detailed(options);
	let results = run_rule(&rule, &lint_file(&heading, Vec::new()), &config);

	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset header must be at most 60 characters" }),
		"explicit max_length should be enforced, got: {results:?}"
	);

	let paragraph = "a".repeat(61);
	let paragraph_results = run_rule(&rule, &lint_file(&paragraph, Vec::new()), &config);
	assert!(
		paragraph_results
			.iter()
			.any(|result| { result.message == "changeset summary must be at most 60 characters" }),
		"explicit max_length should be enforced for plain summaries, got: {paragraph_results:?}"
	);
}

#[test]
fn summary_rule_require_description_passes_with_paragraph_after_heading() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("require_description".to_string(), json!(true));
	let body = "# Fix CLI version flag\n\nThe root clap command was missing.";
	let results = run_rule(&rule, &lint_file(body, Vec::new()), &detailed(options));

	assert!(
		results
			.iter()
			.all(|result| { !result.message.contains("description") }),
		"should pass when description follows heading, got: {results:?}"
	);
}

#[test]
fn summary_rule_require_description_fails_when_only_heading() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("require_description".to_string(), json!(true));
	let results = run_rule(
		&rule,
		&lint_file("# Fix CLI version flag", Vec::new()),
		&detailed(options),
	);

	assert!(results.iter().any(|result| {
		result.message == "changeset summary must be followed by a description paragraph"
	}));
}

#[test]
fn summary_rule_require_description_fails_when_only_empty_lines_after_heading() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("require_description".to_string(), json!(true));
	let body = "# Fix CLI version flag\n\n\n";
	let results = run_rule(&rule, &lint_file(body, Vec::new()), &detailed(options));

	assert!(results.iter().any(|result| {
		result.message == "changeset summary must be followed by a description paragraph"
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
fn summary_rule_reports_missing_heading_and_length_issues_together() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("required".to_string(), json!(true));
	options.insert("min_length".to_string(), json!(20));
	let results = run_rule(
		&rule,
		&lint_file("short summary", Vec::new()),
		&detailed(options),
	);

	// Should report both: must start with heading AND too short.
	// Previously only the heading error was reported due to early return.
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset body must start with a summary heading")
	}));
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset summary must be at least 20 characters")
	}));
}

#[test]
fn summary_rule_require_description_skipped_when_disabled() {
	let rule = SummaryRule::new();
	let mut options = BTreeMap::new();
	options.insert("require_description".to_string(), json!(false));
	let results = run_rule(
		&rule,
		&lint_file("# Fix CLI version flag", Vec::new()),
		&detailed(options),
	);

	assert!(
		!results
			.iter()
			.any(|result| { result.message.contains("description") }),
		"should not check for description when disabled"
	);
}

#[test]
fn no_section_headings_rule_reports_unique_change_type_headings() {
	let rule = NoSectionHeadingsRule::new();
	let file = lint_file(
		"# Summary\n\n## feature\n\nDetails",
		vec![
			change("core", Some(BumpSeverity::Patch), Some("feature")),
			change("cli", Some(BumpSeverity::Patch), Some("feature")),
			change("api", Some(BumpSeverity::Patch), None),
		],
	);
	let results = run_rule(&rule, &file, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1);
	assert!(results.iter().any(|result| {
		result.message == "changeset type `feature` must not also be used as a heading"
	}));
	assert!(run_rule(&rule, &file, &severity(LintSeverity::Off)).is_empty());
	assert!(run_rule_with_wrong_parsed(&rule, &severity(LintSeverity::Error)).is_empty());
}

#[test]
fn bump_scope_rule_reports_all_constraints_for_matching_changes() {
	let rule = BumpScopeRule::new(BumpSeverity::Patch);
	let file = lint_file(
		"# Summary\n\n## Forbidden\n\nA body without the requested section.",
		vec![
			change("core", Some(BumpSeverity::Patch), Some("feature")),
			change("cli", Some(BumpSeverity::Minor), Some("feature")),
		],
	);
	let mut options = BTreeMap::new();
	options.insert("required_bump".to_string(), json!("minor"));
	options.insert("required_sections".to_string(), json!(["Motivation", 7]));
	options.insert("forbidden_headings".to_string(), json!(["Forbidden"]));
	options.insert("min_body_chars".to_string(), json!(200));
	options.insert("max_body_chars".to_string(), json!(10));
	options.insert("require_code_block".to_string(), json!(true));

	let results = run_rule(&rule, &file, &detailed(options));
	assert!(results.iter().any(|result| {
		result
			.message
			.contains("changeset type `feature` requires bump `minor`, found `patch`")
	}));
	assert!(
		results
			.iter()
			.any(|result| { result.message == "changeset must include a `Motivation` section" })
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
fn bump_scope_rule_ignores_non_matching_changes_and_accepts_valid_body() {
	let rule = BumpScopeRule::new(BumpSeverity::Major);
	let file = lint_file(
		"# Summary\n\n## Motivation\n\n```rust\nlet ok = true;\n```",
		vec![change("core", Some(BumpSeverity::Patch), Some("feature"))],
	);
	let mut options = BTreeMap::new();
	options.insert("required_bump".to_string(), json!("major"));
	options.insert("required_sections".to_string(), json!(["Motivation"]));
	options.insert("forbidden_headings".to_string(), json!(["Forbidden"]));
	options.insert("min_body_chars".to_string(), json!(1));
	options.insert("max_body_chars".to_string(), json!(200));
	options.insert("require_code_block".to_string(), json!(true));
	assert!(run_rule(&rule, &file, &detailed(options)).is_empty());
	assert!(run_rule(&rule, &file, &severity(LintSeverity::Off)).is_empty());
	assert!(run_rule_with_wrong_parsed(&rule, &severity(LintSeverity::Error)).is_empty());
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
fn prefer_inline_rule_reports_block_bump_and_type_redundancy() {
	let contents = "---\ncore:\n  bump: minor\n  type: feat\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	let result = results.first().expect("result");
	assert!(result.message.contains("use the inline form `core: feat`"));
	assert_eq!(result.location.line, 1);
	assert_eq!(result.location.column, 1);
	assert_eq!(result.location.span, Some((9, 36)));
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: feat\n---\n\n#### Summary\n"
	);
	assert!(
		run_prefer_inline(
			contents,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Off)
		)
		.is_empty()
	);
}

#[test]
fn prefer_inline_rule_reports_type_only_entries() {
	let contents = "---\ncore:\n  type: feat\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert!(
		results
			.first()
			.expect("result")
			.message
			.contains("only declares `type`")
	);
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: feat\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_reports_flow_mapping_entries() {
	let contents = "---\ncore: {bump: minor, type: feat}\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: feat\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_reports_quoted_flow_keys() {
	let contents = "---\n\"@scope/core\": {type: feat}\n---\n\n#### Summary\n";
	let mut target_types = BTreeMap::new();
	target_types.insert(
		"@scope/core".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("feat".to_string(), BumpSeverity::Minor)]),
		},
	);
	let results = run_prefer_inline(contents, target_types, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\n\"@scope/core\": feat\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_keeps_meaningful_object_entries() {
	// Explicit bumps that disagree with the type default are meaningful.
	let bump_mismatch = "---\ncore:\n  bump: minor\n  type: fix\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			bump_mismatch,
			target_types(&[("feat", BumpSeverity::Minor), ("fix", BumpSeverity::Patch)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);
	// Bare bump entries gain a change type when converted inline.
	let bare_bump = "---\ncore:\n  bump: minor\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			bare_bump,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);
	// Explicit versions cannot be proven redundant by the linter.
	let explicit_version =
		"---\ncore:\n  bump: minor\n  type: feat\n  version: \"1.2.0\"\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			explicit_version,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);
	// `caused_by` has no inline representation.
	let caused_by = "---\ncore:\n  type: feat\n  caused_by: [cli]\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			caused_by,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);
	// Unknown fields are invalid changesets; leave them alone.
	let unknown_field = "---\ncore:\n  type: feat\n  extra: true\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			unknown_field,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);
	// Inline scalar entries are already preferred.
	let inline = "---\ncore: feat\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			inline,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);
}

#[test]
fn prefer_inline_rule_requires_valid_types_for_known_targets() {
	// Unknown types would break the inline form for configured targets.
	let contents = "---\ncore:\n  type: mystery\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			contents,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);

	// Unknown targets keep the type but lose explicit bumps inline.
	let unknown_target_with_bump =
		"---\nmystery:\n  bump: minor\n  type: feat\n---\n\n#### Summary\n";
	assert!(
		run_prefer_inline(
			unknown_target_with_bump,
			target_types(&[("feat", BumpSeverity::Minor)]),
			&severity(LintSeverity::Error)
		)
		.is_empty()
	);

	let unknown_target_type_only = "---\nmystery:\n  type: feat\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		unknown_target_type_only,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(unknown_target_type_only, &results),
		"---\nmystery: feat\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_skips_duplicate_target_keys() {
	// The YAML parser rejects duplicate keys, so ambiguous entries never
	// reach the rule; the changeset is simply not linted.
	let contents = "---\ncore:\n  type: feat\ncore: feat\n---\n\n#### Summary\n";
	assert!(parse_changeset_for_lint(contents).is_none());
}

#[test]
fn prefer_inline_rule_skips_comments_inside_block_values() {
	let contents = "---\ncore:\n  # note\n  type: feat\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert!(results.is_empty(), "unexpected results: {results:?}");
}

#[test]
fn prefer_inline_rule_skips_wrong_parsed_targets() {
	let rule = PreferInlineRule::new();
	assert!(run_rule_with_wrong_parsed(&rule, &severity(LintSeverity::Error)).is_empty());
}

#[test]
fn prefer_inline_rule_handles_blocks_before_and_after_other_entries() {
	let contents =
		"---\ncli: feat\ncore:\n  bump: minor\n  type: feat\nother: patch\n---\n\n#### Summary\n";
	let mut target_types = target_types(&[("feat", BumpSeverity::Minor)]);
	target_types.insert(
		"other".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("patch".to_string(), BumpSeverity::Patch)]),
		},
	);
	let results = run_prefer_inline(contents, target_types, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncli: feat\ncore: feat\nother: patch\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_quotes_unsafe_inline_tokens() {
	let contents = "---\ncore:\n  type: \"@weird type\"\n---\n\n#### Summary\n";
	let mut target_types = target_types(&[]);
	target_types.insert(
		"core".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("@weird type".to_string(), BumpSeverity::Minor)]),
		},
	);
	let results = run_prefer_inline(contents, target_types, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: \"@weird type\"\n---\n\n#### Summary\n"
	);
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
			publish: monochange_core::PublishSettings::default(),
		});
	configuration.groups.push(monochange_core::GroupDefinition {
		id: "group".to_string(),
		packages: vec!["core".to_string()],
		package_max_bumps: BTreeMap::new(),
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

#[test]
fn normalize_change_bumps_resolves_implied_bumps_for_configured_types() {
	let mut file = parse_changeset_for_lint(
		"---\ncore: feat\nmystery: feat\nempty: ''\n---\n\n#### Add target\n",
	)
	.expect("changeset should parse");
	file.target_types = target_types(&[("feat", BumpSeverity::Minor)]);

	assert!(
		file.changes
			.iter()
			.all(|entry| entry.package != "core" || entry.bump.is_none())
	);
	normalize_change_bumps(&mut file);

	let core = file
		.changes
		.iter()
		.find(|entry| entry.package == "core")
		.expect("core entry");
	assert_eq!(core.bump, Some(BumpSeverity::Minor));
	// Unknown targets keep no implied bump, matching release planning.
	let mystery = file
		.changes
		.iter()
		.find(|entry| entry.package == "mystery")
		.expect("mystery entry");
	assert_eq!(mystery.bump, None);
	// Entries without a change type keep their bump untouched.
	let empty = file
		.changes
		.iter()
		.find(|entry| entry.package == "empty")
		.expect("empty entry");
	assert_eq!(empty.bump, None);
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
fn frontmatter_entry_spans_reports_block_and_flow_spans() {
	let contents = "---\ncore:\n  type: feat\nother: {type: fix}\n---\n";
	let spans = spans_of(contents).expect("entries");
	assert_eq!(spans.len(), 2);
	assert_eq!(spans[0].0, "core");
	assert_eq!(spans[0].1, (9, 22));
	assert_eq!(spans[1].0, "other");
	assert_eq!(spans[1].1, (29, 41));
}

#[test]
fn frontmatter_entry_spans_bails_out_on_ambiguous_inputs() {
	// Duplicate keys make span attribution ambiguous, even when only one
	// of the duplicates is a mapping.
	assert!(spans_of("---\ncore: a\ncore: b\n---\n").is_none());
	assert!(spans_of("---\ncore:\n  type: feat\ncore: feat\n---\n").is_none());
	// A leading line without a `key:` shape cannot be attributed.
	assert!(spans_of("---\njusttext\n---\n").is_none());
	// YAML explicit keys are not attributed to spans.
	assert!(spans_of("---\n? core\n: feat\n---\n").is_none());
}

#[test]
fn frontmatter_entry_spans_skips_scalar_and_comment_values() {
	let contents = "---\ncore: feat\n# comment\nother:\n---\n";
	let spans = spans_of(contents).expect("entries");
	assert!(spans.is_empty(), "unexpected spans: {spans:?}");
}

#[test]
fn frontmatter_entry_spans_handles_quoted_keys() {
	let contents = "---\n\"core\":\n  type: feat\n'other':\n  type: fix\n---\n";
	let spans = spans_of(contents).expect("entries");
	assert_eq!(spans.len(), 2);
	assert_eq!(spans[0].0, "core");
	assert_eq!(spans[0].1, (11, 24));
	assert_eq!(spans[1].0, "other");
}

#[test]
fn frontmatter_entry_spans_rejects_malformed_keys() {
	// Quoted key with no colon after the closing quote.
	assert!(spans_of("---\n\"core\" feat\n---\n").is_none());
	// Quoted key followed by a bare value.
	assert!(spans_of("---\n\"core\"feat\n---\n").is_none());
	// A top-level key without a colon terminator.
	assert!(spans_of("---\ncore\n---\n").is_none());
	// A key:value pair without whitespace after the colon.
	assert!(spans_of("---\ncore:feat\n---\n").is_none());
	// Empty top-level key.
	assert!(spans_of("---\n: v\n---\n").is_none());
}

#[test]
fn frontmatter_entry_spans_skips_unrewritable_flow_values() {
	// Unbalanced flow mappings produce no entries.
	assert!(
		spans_of("---\ncore: {type: feat\n---\n")
			.expect("entries")
			.is_empty()
	);
	// Flow mappings followed by trailing content are not rewritten.
	assert!(
		spans_of("---\ncore: {type: feat} trailing\n---\n")
			.expect("entries")
			.is_empty()
	);
}

#[test]
fn frontmatter_entry_spans_scans_flow_values_across_quotes() {
	let contents = "---\ncore: {type: \"feat\"}\n---\n";
	let spans = spans_of(contents).expect("entries");
	assert_eq!(spans.len(), 1);
	assert_eq!(spans[0].1, (9, 24));

	// Nested braces stay balanced.
	let nested = "---\ncore: {caused: {a: 1}, type: feat}\n---\n";
	let spans = spans_of(nested).expect("entries");
	assert_eq!(spans[0].1, (9, 38));

	// Braces inside quotes do not affect nesting.
	let quoted = "---\ncore: {type: \"}\"}\n---\n";
	let spans = spans_of(quoted).expect("entries");
	assert_eq!(spans[0].1, (9, 21));
}

#[test]
fn prefer_inline_rule_reports_explicit_keys_as_ambiguous() {
	// YAML explicit keys parse fine in serde but cannot be attributed to raw
	// spans; the rule skips the file instead of rewriting them.
	let contents = "---\n? core\n: feat\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert!(results.is_empty(), "unexpected results: {results:?}");
}

#[test]
fn prefer_inline_rule_quotes_yaml_special_tokens() {
	// Tokens that could be misread as other YAML scalars are quoted.
	let contents = "---\ncore:\n  type: no\n---\n\n#### Summary\n";
	let mut target_types = target_types(&[]);
	target_types.insert(
		"core".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("no".to_string(), BumpSeverity::Minor)]),
		},
	);
	let results = run_prefer_inline(contents, target_types, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: \"no\"\n---\n\n#### Summary\n"
	);
}

#[test]
fn prefer_inline_rule_handles_single_quoted_keys() {
	let contents = "---\n'@scope/core': {type: feat}\n---\n\n#### Summary\n";
	let mut target_types = BTreeMap::new();
	target_types.insert(
		"@scope/core".to_string(),
		TargetChangeTypes {
			default_bumps: BTreeMap::from([("feat".to_string(), BumpSeverity::Minor)]),
		},
	);
	let results = run_prefer_inline(contents, target_types, &severity(LintSeverity::Error));
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\n'@scope/core': feat\n---\n\n#### Summary\n"
	);
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
fn prefer_inline_rule_reports_entries_with_blank_lines_in_blocks() {
	let contents = "---\ncore:\n  bump: minor\n\n  type: feat\n---\n\n#### Summary\n";
	let results = run_prefer_inline(
		contents,
		target_types(&[("feat", BumpSeverity::Minor)]),
		&severity(LintSeverity::Error),
	);
	assert_eq!(results.len(), 1, "unexpected results: {results:?}");
	assert_eq!(
		apply_single_fix(contents, &results),
		"---\ncore: feat\n---\n\n#### Summary\n"
	);
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
fn frontmatter_entry_spans_rejects_truncated_and_mismatched_quoted_keys() {
	// An escape with no following character cannot close the key.
	assert!(spans_of("---\n\"core\\\n---\n").is_none());
	// A quoted key whose colon is followed by a bare value.
	assert!(spans_of("---\n\"core\":feat\n---\n").is_none());
}
