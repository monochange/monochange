#![forbid(clippy::indexing_slicing)]

//! Changeset lint suite for monochange.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use monochange_core::BumpSeverity;
use monochange_core::ChangelogSettings;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintCategory;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintMaturity;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_core::lint::LintTargetMetadata;

use crate::RawChangeEntry;
use crate::parse_bump_severity;

/// Return the shared changeset lint suite.
#[must_use]
pub fn lint_suite() -> ChangesetLintSuite {
	ChangesetLintSuite::new()
}

/// Parsed changeset data stored in a lint target.
#[derive(Debug, Clone)]
pub struct ChangesetLintFile {
	/// The markdown body after frontmatter.
	pub(crate) body: String,
	/// The parsed change entries from frontmatter.
	pub(crate) changes: Vec<RawChangeEntry>,
	/// The raw frontmatter values per target, in declaration order.
	pub(crate) raw_values: Vec<(String, serde_yaml_ng::Value)>,
	/// Change-type metadata per configured target id.
	pub(crate) target_types: BTreeMap<String, TargetChangeTypes>,
}

/// Change-type metadata for a configured package or group target.
#[derive(Debug, Clone, Default)]
pub struct TargetChangeTypes {
	/// Valid change types for the target mapped to the bump severity they imply.
	pub(crate) default_bumps: BTreeMap<String, BumpSeverity>,
}

/// Changeset lint suite implementation.
#[derive(Debug, Clone, Default)]
pub struct ChangesetLintSuite;

impl ChangesetLintSuite {
	/// Create a new changeset lint suite.
	#[must_use]
	pub fn new() -> Self {
		Self
	}
}

impl LintSuite for ChangesetLintSuite {
	fn suite_id(&self) -> &'static str {
		"changesets"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(SummaryRule::new()),
			Box::new(NoSectionHeadingsRule::new()),
			Box::new(PreferInlineRule::new()),
			Box::new(BumpScopeRule::new(BumpSeverity::None)),
			Box::new(BumpScopeRule::new(BumpSeverity::Patch)),
			Box::new(BumpScopeRule::new(BumpSeverity::Minor)),
			Box::new(BumpScopeRule::new(BumpSeverity::Major)),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"changesets/recommended",
				"Changesets recommended",
				"Balanced changeset linting for typical monochange repositories",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([
				(
					"changesets/summary".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"changesets/prefer-inline".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
			])),
		]
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		let changeset_dir = workspace_root.join(".changeset");
		if !changeset_dir.exists() {
			return Ok(Vec::new());
		}

		let mut targets = Vec::new();
		for entry in fs::read_dir(&changeset_dir)
			.map_err(|error| {
				MonochangeError::Io(format!("failed to read changeset directory: {error}"))
			})?
			.filter_map(Result::ok)
		{
			let path = entry.path();
			let Some(ext) = path.extension() else {
				continue;
			};
			if ext != "md" {
				continue;
			}
			let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
			if !Path::new(file_name)
				.extension()
				.is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
				|| file_name == "README.md"
			{
				continue;
			}

			let contents = fs::read_to_string(&path).map_err(|error| {
				MonochangeError::Io(format!("failed to read changeset file: {error}"))
			})?;
			let Some(mut file) = parse_changeset_for_lint(&contents) else {
				continue;
			};
			file.target_types = target_change_types(configuration);
			normalize_change_bumps(&mut file);

			let relative_path = path.strip_prefix(workspace_root).unwrap_or(&path);
			targets.push(LintTarget::new(
				workspace_root.to_path_buf(),
				path.clone(),
				contents,
				LintTargetMetadata {
					ecosystem: "changesets".to_string(),
					relative_path: relative_path.to_path_buf(),
					package_name: None,
					package_id: None,
					group_id: None,
					managed: false,
					private: None,
					publishable: None,
				},
				Box::new(file),
			));
		}

		Ok(targets)
	}
}

/// Collect the valid change types and their default bumps per configured
/// package and group target.
fn target_change_types(
	configuration: &WorkspaceConfiguration,
) -> BTreeMap<String, TargetChangeTypes> {
	let mut target_types = BTreeMap::new();
	for package in &configuration.packages {
		target_types.insert(
			package.id.clone(),
			TargetChangeTypes {
				default_bumps: change_type_default_bumps(
					&configuration.changelog,
					&package.excluded_changelog_types,
				),
			},
		);
	}
	for group in &configuration.groups {
		target_types.insert(
			group.id.clone(),
			TargetChangeTypes {
				default_bumps: change_type_default_bumps(
					&configuration.changelog,
					&group.excluded_changelog_types,
				),
			},
		);
	}
	target_types
}

/// Resolve the bumps implied by configured change types so lint rules observe
/// the same bumps release planning computes for inline and `type`-only entries.
fn normalize_change_bumps(file: &mut ChangesetLintFile) {
	for change in &mut file.changes {
		if change.bump.is_some() {
			continue;
		}
		let Some(change_type) = change.change_type.as_deref() else {
			continue;
		};
		if let Some(default_bump) = file
			.target_types
			.get(change.package.as_str())
			.and_then(|target| target.default_bumps.get(change_type))
		{
			change.bump = Some(*default_bump);
		}
	}
}

fn change_type_default_bumps(
	changelog: &ChangelogSettings,
	excluded_types: &[String],
) -> BTreeMap<String, BumpSeverity> {
	let excluded: BTreeSet<&str> = excluded_types.iter().map(String::as_str).collect();
	changelog
		.types
		.iter()
		.filter(|(name, _)| !excluded.contains(name.as_str()))
		.map(|(name, change_type)| (name.clone(), change_type.bump))
		.collect()
}

/// Parse a changeset file for linting.
///
/// Returns `Some(file)` if the file has valid frontmatter,
/// or `None` if it doesn't look like a changeset file.
fn parse_changeset_for_lint(contents: &str) -> Option<ChangesetLintFile> {
	let contents = contents.replace("\r\n", "\n").replace('\r', "\n");
	let without_opening = contents.strip_prefix("---")?;
	let (frontmatter, body_with_separator) = without_opening.split_once("\n---\n")?;
	let body = body_with_separator.trim().to_string();
	let mapping: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(frontmatter).ok()?;

	let mut changes = Vec::new();
	let mut raw_values = Vec::new();
	for (key, value) in mapping {
		let package = key.as_str()?;
		let (bump, change_type) = parse_simple_change_value(&value);
		raw_values.push((package.to_string(), value));
		changes.push(RawChangeEntry {
			package: package.to_string(),
			bump,
			version: None,
			reason: None,
			details: None,
			change_type,
			caused_by: Vec::new(),
		});
	}

	Some(ChangesetLintFile {
		body,
		changes,
		raw_values,
		target_types: BTreeMap::new(),
	})
}

fn parse_simple_change_value(
	value: &serde_yaml_ng::Value,
) -> (Option<BumpSeverity>, Option<String>) {
	if let Some(token) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
		if let Some(bump) = parse_bump_severity(token) {
			return (Some(bump), None);
		}
		return (None, Some(token.to_string()));
	}

	if let Some(mapping) = value.as_mapping() {
		let bump = mapping
			.get(serde_yaml_ng::Value::String("bump".to_string()))
			.and_then(serde_yaml_ng::Value::as_str)
			.and_then(parse_bump_severity);
		let change_type = mapping
			.get(serde_yaml_ng::Value::String("type".to_string()))
			.and_then(serde_yaml_ng::Value::as_str)
			.map(str::trim)
			.filter(|s| !s.is_empty())
			.map(ToString::to_string);
		return (bump, change_type);
	}

	(None, None)
}

fn changeset_file<'a>(ctx: &'a LintContext<'a>) -> Option<&'a ChangesetLintFile> {
	ctx.parsed_as::<ChangesetLintFile>()
}

// ── Summary rule ───────────────────────────────────────────────────────────

#[derive(Debug)]
struct SummaryRule {
	rule: LintRule,
}

impl SummaryRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"changesets/summary",
				"Changeset summary heading",
				"Requires changeset body to start with a summary heading",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for SummaryRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		let required = config.bool_option("required", false);
		let heading_level = config
			.option("heading_level")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let min_length = config
			.option("min_length")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let max_heading_length = config
			.option("max_heading_length")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize)
			.or(Some(60));
		let max_length = config
			.option("max_length")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let require_description = config.bool_option("require_description", false);
		let forbid_trailing_period = config.bool_option("forbid_trailing_period", false);
		let forbid_conventional_commit_prefix =
			config.bool_option("forbid_conventional_commit_prefix", false);

		use crate::first_non_empty_line;
		use crate::has_conventional_commit_prefix;
		use crate::markdown_heading_level;
		use crate::markdown_heading_text;

		let mut results = Vec::new();
		let body = &file.body;

		let Some(first_line) = first_non_empty_line(body) else {
			if required {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					"changeset body must start with a summary heading",
					severity,
				));
			}
			return results;
		};

		let heading = markdown_heading_level(first_line);
		if required && heading.is_none() {
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				"changeset body must start with a summary heading",
				severity,
			));
		}

		if let (Some(required_level), Some(actual_level)) = (heading_level, heading)
			&& actual_level != required_level
		{
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				format!(
					"changeset summary heading must use level {required_level}, found level {actual_level}"
				),
				severity,
			));
		}

		let summary =
			markdown_heading_text(first_line).unwrap_or_else(|| first_line.trim().to_string());

		if let Some(min) = min_length
			&& summary.chars().count() < min
		{
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				format!("changeset summary must be at least {min} characters"),
				severity,
			));
		}

		let summary_is_heading = heading.is_some();
		let effective_max_length = if summary_is_heading {
			max_length.or(max_heading_length)
		} else {
			max_length
		};
		if let Some(max) = effective_max_length
			&& summary.chars().count() > max
		{
			let subject = if summary_is_heading {
				"changeset header"
			} else {
				"changeset summary"
			};
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				format!("{subject} must be at most {max} characters"),
				severity,
			));
		}

		if forbid_trailing_period && summary.ends_with('.') {
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				"changeset summary must not end with a period",
				severity,
			));
		}

		if forbid_conventional_commit_prefix && has_conventional_commit_prefix(&summary) {
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				"changeset summary must not use a conventional-commit prefix",
				severity,
			));
		}

		if require_description {
			let has_description = body
				.lines()
				.skip(1) // skip the heading line itself
				.any(|line| {
					let trimmed = line.trim();
					!trimmed.is_empty() && !trimmed.starts_with('#')
				});

			if !has_description {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					"changeset summary must be followed by a description paragraph",
					severity,
				));
			}
		}

		results
	}
}

// ── No section headings rule ─────────────────────────────────────────────────

#[derive(Debug)]
struct NoSectionHeadingsRule {
	rule: LintRule,
}

impl NoSectionHeadingsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"changesets/no_section_headings",
				"Changeset no section headings",
				"Requires changeset body to not use change types as headings",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for NoSectionHeadingsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		use std::collections::BTreeSet;

		use crate::markdown_has_heading;

		let change_types: BTreeSet<&str> = file
			.changes
			.iter()
			.filter_map(|change| change.change_type.as_deref())
			.collect();

		let mut results = Vec::new();
		for change_type in change_types {
			if markdown_has_heading(&file.body, change_type) {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("changeset type `{change_type}` must not also be used as a heading"),
					severity,
				));
			}
		}

		results
	}
}

// ── Bump scope rule ────────────────────────────────────────────────────────

#[derive(Debug)]
struct BumpScopeRule {
	rule: LintRule,
	bump: BumpSeverity,
}

impl BumpScopeRule {
	fn new(bump: BumpSeverity) -> Self {
		Self {
			rule: LintRule::new(
				format!("changesets/bump/{bump}"),
				format!("Changeset {bump} scope"),
				format!("Requires changesets with bump `{bump}` to satisfy scope rules"),
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
			bump,
		}
	}
}

impl LintRuleRunner for BumpScopeRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		use crate::markdown_has_code_block;
		use crate::markdown_has_heading;

		let required_bump = config
			.option("required_bump")
			.and_then(|v| v.as_str())
			.and_then(parse_bump_severity);
		let required_sections = config
			.string_list_option("required_sections")
			.unwrap_or_default();
		let forbidden_headings = config
			.string_list_option("forbidden_headings")
			.unwrap_or_default();
		let min_body_chars = config
			.option("min_body_chars")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let max_body_chars = config
			.option("max_body_chars")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let require_code_block = config.bool_option("require_code_block", false);

		let mut results = Vec::new();

		for change in &file.changes {
			if change.bump != Some(self.bump) {
				continue;
			}

			if let Some(required) = required_bump
				&& change.bump != Some(required)
			{
				let actual = change
					.bump
					.map_or_else(|| "auto".to_string(), |b| b.to_string());
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!(
						"changeset type `{}` requires bump `{required}`, found `{actual}`",
						change.change_type.as_deref().unwrap_or("<unknown>")
					),
					severity,
				));
			}

			for section in &required_sections {
				if !markdown_has_heading(&file.body, section) {
					results.push(LintResult::new(
						self.rule.id.clone(),
						LintLocation::new(ctx.manifest_path, 1, 1),
						format!("changeset must include a `{section}` section"),
						severity,
					));
				}
			}

			for heading in &forbidden_headings {
				if markdown_has_heading(&file.body, heading) {
					results.push(LintResult::new(
						self.rule.id.clone(),
						LintLocation::new(ctx.manifest_path, 1, 1),
						format!("changeset must not use `{heading}` as a heading"),
						severity,
					));
				}
			}

			if let Some(min_chars) = min_body_chars
				&& file.body.trim().chars().count() < min_chars
			{
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("changeset body must be at least {min_chars} characters"),
					severity,
				));
			}

			if let Some(max_chars) = max_body_chars
				&& file.body.trim().chars().count() > max_chars
			{
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("changeset body must be at most {max_chars} characters"),
					severity,
				));
			}

			if require_code_block && !markdown_has_code_block(&file.body) {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					"changeset must include a fenced code block",
					severity,
				));
			}
		}

		results
	}
}

// ── Prefer inline rule ────────────────────────────────────────────────────

#[derive(Debug)]
struct PreferInlineRule {
	rule: LintRule,
}

impl PreferInlineRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"changesets/prefer-inline",
				"Changeset prefer inline entries",
				"Prefers inline `target: type` change entries when the object form only repeats what the inline form already implies",
				LintCategory::Style,
				LintMaturity::Stable,
				true,
			),
		}
	}
}

impl LintRuleRunner for PreferInlineRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		let Some(entries) = frontmatter_entry_spans(ctx.contents) else {
			return Vec::new();
		};
		let mut span_by_target: BTreeMap<String, FrontmatterValueSpan> = BTreeMap::new();
		for entry in entries {
			if span_by_target
				.insert(entry.package.clone(), entry.span)
				.is_some()
			{
				// Duplicate keys make span attribution ambiguous.
				return Vec::new();
			}
		}

		let mut results = Vec::new();
		for (package, value) in &file.raw_values {
			let Some(span) = span_by_target.get(package.as_str()) else {
				continue;
			};
			if !span.is_mapping {
				continue;
			}
			let Some(token) = inline_equivalent_token(package, value, &file.target_types) else {
				continue;
			};
			let message = prefer_inline_message(package, value, &token);
			results.push(
				LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1)
						.with_span(span.value_span.0, span.value_span.1),
					message,
					severity,
				)
				.with_fix(LintFix::single(
					"Convert change entry to inline form",
					span.value_span,
					format!(" {}", inline_scalar_token(&token)),
				)),
			);
		}

		results
	}
}

/// Decide whether a raw frontmatter value is exactly equivalent to the inline
/// `target: token` form, returning the inline token when it is.
fn inline_equivalent_token(
	package: &str,
	value: &serde_yaml_ng::Value,
	target_types: &BTreeMap<String, TargetChangeTypes>,
) -> Option<String> {
	let mapping = value.as_mapping()?;
	let mut bump: Option<BumpSeverity> = None;
	let mut change_type: Option<String> = None;
	for (key, field) in mapping {
		match key.as_str()? {
			"bump" => {
				let token = field
					.as_str()
					.map(str::trim)
					.filter(|token| !token.is_empty())?;
				bump = Some(parse_bump_severity(token)?);
			}
			"type" => {
				let token = field
					.as_str()
					.map(str::trim)
					.filter(|token| !token.is_empty())?;
				change_type = Some(token.to_string());
			}
			// `version` and `caused_by` have no inline representation, and
			// proving an explicit version is redundant would require the
			// release-planning version context that linting does not load.
			_ => return None,
		}
	}

	let token = change_type?;
	match target_types.get(package) {
		Some(target) => {
			// For configured targets the inline form only accepts valid types.
			let default_bump = target.default_bumps.get(token.as_str())?;
			if bump.is_some_and(|explicit| explicit != *default_bump) {
				return None;
			}
		}
		None => {
			// Unknown targets keep the type but lose any explicit bump inline.
			if bump.is_some() {
				return None;
			}
		}
	}
	Some(token)
}

fn prefer_inline_message(package: &str, value: &serde_yaml_ng::Value, token: &str) -> String {
	let repeats_bump = value
		.as_mapping()
		.and_then(|mapping| mapping.get(serde_yaml_ng::Value::String("bump".to_string())))
		.and_then(serde_yaml_ng::Value::as_str)
		.and_then(parse_bump_severity)
		.is_some();
	if repeats_bump {
		format!(
			"changeset entry for `{package}` repeats a bump that type `{token}` already implies; use the inline form `{package}: {token}`"
		)
	} else {
		format!(
			"changeset entry for `{package}` only declares `type`; use the inline form `{package}: {token}`"
		)
	}
}

/// Render the inline token as a safe YAML scalar.
fn inline_scalar_token(token: &str) -> String {
	let plain_safe = token
		.chars()
		.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '+' | '@'))
		&& token
			.chars()
			.next()
			.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
		&& !matches!(
			token,
			"true" | "false" | "null" | "~" | "yes" | "no" | "on" | "off"
		);
	if plain_safe {
		token.to_string()
	} else {
		format!("\"{}\"", token.replace('\\', "\\\\").replace('"', "\\\""))
	}
}

/// The raw value span of one top-level frontmatter entry.
struct FrontmatterValueSpan {
	/// Byte span covering the value region an inline conversion replaces.
	value_span: (usize, usize),
	/// Whether the raw value is a YAML mapping.
	is_mapping: bool,
}

struct FrontmatterEntrySpan {
	package: String,
	span: FrontmatterValueSpan,
}

/// Locate the top-level entries of a changeset frontmatter and the raw value
/// spans needed to rewrite them inline.
///
/// Returns `None` when the contents do not look like a changeset file; rules
/// should skip the file in that case.
fn frontmatter_entry_spans(contents: &str) -> Option<Vec<FrontmatterEntrySpan>> {
	let without_opening = contents.strip_prefix("---")?;
	let (frontmatter, _) = without_opening.split_once("\n---\n")?;
	// `frontmatter` keeps the newline that follows the opening `---`.
	let lines = collect_frontmatter_lines(frontmatter, 3);

	let mut entries = Vec::new();
	let mut index = 0usize;
	while let Some((line_start, line)) = lines.get(index) {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			index += 1;
			continue;
		}
		let indent = line.len() - line.trim_start_matches(' ').len();
		if indent > 0 {
			// Continuation line without a parent entry; ignore defensively.
			index += 1;
			continue;
		}

		let (package, after_colon) = parse_frontmatter_entry_line(line)?;
		let inline_value = line.get(after_colon..).unwrap_or("").trim_start();
		if inline_value.is_empty() || inline_value.starts_with('#') {
			let (span, next_index) = block_value_span(&lines, index, *line_start, after_colon);
			if let Some(span) = span {
				entries.push(FrontmatterEntrySpan { package, span });
			}
			index = next_index;
			continue;
		}

		if inline_value.starts_with('{') {
			let value_start = line_start + line.len() - inline_value.len();
			if let Some(value_end) = flow_mapping_end(contents, value_start) {
				entries.push(FrontmatterEntrySpan {
					package,
					span: FrontmatterValueSpan {
						// Start right after the colon so the fix aligns with the
						// block-form rewrite below.
						value_span: (line_start + after_colon, value_end),
						is_mapping: true,
					},
				});
			}
			index += 1;
			continue;
		}

		// Inline scalar values are already in the preferred form.
		index += 1;
	}

	Some(entries)
}

/// Collect frontmatter lines with their absolute byte offsets.
fn collect_frontmatter_lines(frontmatter: &str, base: usize) -> Vec<(usize, String)> {
	let mut lines = Vec::new();
	let mut cursor = 0usize;
	while cursor < frontmatter.len() {
		if !frontmatter.is_char_boundary(cursor) {
			return lines;
		}
		let Some(remainder) = frontmatter.get(cursor..) else {
			return lines;
		};
		let (raw_line, advance) = match remainder.split_once('\n') {
			Some((line, _)) => (line, line.len() + 1),
			None => (remainder, remainder.len()),
		};
		lines.push((base + cursor, raw_line.trim_end_matches('\r').to_string()));
		cursor += advance;
	}
	lines
}

/// Compute the byte span of a block mapping value that starts after the
/// entry's colon, returning the span (when the block is safely rewritable)
/// and the index of the first line after the block.
fn block_value_span(
	lines: &[(usize, String)],
	entry_index: usize,
	entry_line_start: usize,
	after_colon: usize,
) -> (Option<FrontmatterValueSpan>, usize) {
	let mut value_end: Option<usize> = None;
	let mut index = entry_index + 1;
	while let Some((line_start, line)) = lines.get(index) {
		let trimmed = line.trim();
		if trimmed.is_empty() {
			index += 1;
			continue;
		}
		let indent = line.len() - line.trim_start_matches(' ').len();
		if indent == 0 {
			break;
		}
		if trimmed.starts_with('#') {
			// Comments inside the block are not safe to rewrite.
			return (None, index + 1);
		}
		value_end = Some(line_start + line.len());
		index += 1;
	}
	let span = value_end.map(|end| {
		FrontmatterValueSpan {
			value_span: (entry_line_start + after_colon, end),
			is_mapping: true,
		}
	});
	(span, index)
}

/// Find the byte offset just past the closing brace of a flow mapping that
/// starts at `start`, or `None` when the mapping is unbalanced or is followed
/// by unexpected trailing content on the same line.
fn flow_mapping_end(contents: &str, start: usize) -> Option<usize> {
	let remainder = contents.get(start..)?;
	let mut depth = 0usize;
	let mut in_single_quote = false;
	let mut in_double_quote = false;
	let mut escaped = false;
	let mut chars = remainder.char_indices().peekable();
	while let Some((offset, ch)) = chars.next() {
		if in_double_quote {
			if escaped {
				escaped = false;
			} else if ch == '\\' {
				escaped = true;
			} else if ch == '"' {
				in_double_quote = false;
			}
			continue;
		}
		if in_single_quote {
			if ch == '\'' {
				if chars.peek().map(|(_, next)| *next) == Some('\'') {
					chars.next();
				} else {
					in_single_quote = false;
				}
			}
			continue;
		}
		match ch {
			'"' => in_double_quote = true,
			'\'' => in_single_quote = true,
			'{' => depth += 1,
			'}' => {
				depth = depth.saturating_sub(1);
				if depth == 0 {
					let end = start + offset + ch.len_utf8();
					let rest = contents.get(end..).unwrap_or("");
					let line_rest = rest.split('\n').next().unwrap_or("");
					let trimmed = line_rest.trim();
					if trimmed.is_empty() || trimmed.starts_with('#') {
						return Some(end);
					}
					return None;
				}
			}
			_ => {}
		}
	}
	None
}

/// Parse a top-level frontmatter entry line into its key and the byte index
/// just past the `:` terminator.
fn parse_frontmatter_entry_line(line: &str) -> Option<(String, usize)> {
	match line.chars().next()? {
		'"' | '\'' => parse_quoted_entry_key(line),
		_ => parse_plain_entry_key(line),
	}
}

fn parse_plain_entry_key(line: &str) -> Option<(String, usize)> {
	let mut colon: Option<usize> = None;
	for (offset, ch) in line.char_indices() {
		if ch != ':' {
			continue;
		}
		let followed_by_space = line
			.get(offset + 1..)
			.is_none_or(|rest| rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t'));
		if followed_by_space {
			colon = Some(offset);
			break;
		}
	}
	let colon = colon?;
	let key = line.get(..colon)?.trim().to_string();
	if key.is_empty() {
		return None;
	}
	let after_colon = colon + 1;
	let rest = line.get(after_colon..).unwrap_or("");
	if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
		return None;
	}
	Some((key, after_colon))
}

fn parse_quoted_entry_key(line: &str) -> Option<(String, usize)> {
	let quote = line.chars().next()?;
	let body = line.get(1..)?;
	let mut key = String::new();
	let mut closed_at: Option<usize> = None;
	let mut chars = body.char_indices().peekable();
	while let Some((offset, ch)) = chars.next() {
		if quote == '"' && ch == '\\' {
			match chars.next() {
				Some((_, escaped)) => key.push(escaped),
				None => return None,
			}
		} else if ch == quote {
			if quote == '\'' && chars.peek().map(|(_, next)| *next) == Some('\'') {
				chars.next();
				key.push('\'');
			} else {
				closed_at = Some(offset);
				break;
			}
		} else {
			key.push(ch);
		}
	}
	let closed_at = closed_at?;
	let after_quote = 1 + closed_at + quote.len_utf8();
	let tail = line.get(after_quote..)?;
	let without_indent = tail.trim_start_matches([' ', '\t']);
	if !without_indent.starts_with(':') {
		return None;
	}
	let after_colon = after_quote + (tail.len() - without_indent.len()) + 1;
	let rest = without_indent.get(1..).unwrap_or("");
	if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
		return None;
	}
	Some((key, after_colon))
}

// ── Trait extension for LintRuleConfig ─────────────────────────────────────

#[allow(dead_code)]
trait LintRuleConfigExt {
	fn bool_option(&self, key: &str, default: bool) -> bool;
	fn string_list_option(&self, key: &str) -> Option<Vec<String>>;
}

impl LintRuleConfigExt for LintRuleConfig {
	fn bool_option(&self, key: &str, default: bool) -> bool {
		self.option(key)
			.and_then(serde_json::Value::as_bool)
			.unwrap_or(default)
	}

	fn string_list_option(&self, key: &str) -> Option<Vec<String>> {
		self.option(key)?.as_array().map(|arr| {
			arr.iter()
				.filter_map(|v| v.as_str().map(ToString::to_string))
				.collect()
		})
	}
}

#[cfg(test)]
#[path = "__tests__/lints_tests.rs"]
mod tests;
