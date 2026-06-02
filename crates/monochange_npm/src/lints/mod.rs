#![forbid(clippy::indexing_slicing)]

//! npm-family manifest lint suite.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use monochange_core::MonochangeResult;
use monochange_core::PublishState;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintCategory;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintMaturity;
use monochange_core::lint::LintOptionDefinition;
use monochange_core::lint::LintOptionKind;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_core::lint::LintTargetMetadata;
use monochange_core::relative_to_root;
use serde_json::Map;
use serde_json::Value;

use crate::discover_npm_packages;

/// Return the shared npm-family lint suite.
#[must_use]
pub fn lint_suite() -> NpmLintSuite {
	NpmLintSuite
}

/// npm-family lint suite implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct NpmLintSuite;

#[derive(Debug, Clone)]
struct NpmLintFile {
	manifest: Value,
	workspace_package_names: Arc<BTreeSet<String>>,
}

impl LintSuite for NpmLintSuite {
	fn suite_id(&self) -> &'static str {
		"npm"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(WorkspaceProtocolRule::new()),
			Box::new(SortedDependenciesRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(RootNoProdDepsRule::new()),
			Box::new(NoDuplicateDependenciesRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"npm/recommended",
				"npm recommended",
				"Balanced npm-family manifest linting for typical JavaScript workspaces",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([
				(
					"npm/workspace-protocol".to_string(),
					LintRuleConfig::Severity(LintSeverity::Off),
				),
				(
					"npm/sorted-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
				(
					"npm/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/root-no-prod-deps".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/no-duplicate-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
			])),
			LintPreset::new(
				"npm/strict",
				"npm strict",
				"Opinionated npm-family manifest linting with style rules promoted to errors",
				LintMaturity::Strict,
			)
			.with_rules(BTreeMap::from([
				(
					"npm/workspace-protocol".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/sorted-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/root-no-prod-deps".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/no-duplicate-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
			])),
		]
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		let discovery = discover_npm_packages(workspace_root)?;
		let workspace_package_names = Arc::new(
			discovery
				.packages
				.iter()
				.map(|package| package.name.clone())
				.collect::<BTreeSet<_>>(),
		);

		discovery
			.packages
			.into_iter()
			.filter(|package| {
				is_lintable_workspace_manifest(workspace_root, &package.manifest_path)
			})
			.map(|package| {
				let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
					monochange_core::MonochangeError::IoSource {
						path: package.manifest_path.clone(),
						source: error,
					}
				})?;
				let manifest = serde_json::from_str::<Value>(&contents).map_err(|error| {
					monochange_core::MonochangeError::Parse {
						path: package.manifest_path.clone(),
						source: Box::new(error),
					}
				})?;
				let manifest_dir = package.manifest_path.parent().unwrap_or(workspace_root);
				let configured_package =
					configured_package(configuration, workspace_root, manifest_dir);
				let package_id = configured_package.map(ToString::to_string);
				let group_id = configured_package.and_then(|package_id| {
					configuration
						.group_for_package(package_id)
						.map(|group| group.id.clone())
				});
				let relative_path = relative_to_root(workspace_root, &package.manifest_path)
					.unwrap_or_else(|| package.manifest_path.clone());
				let private = matches!(package.publish_state, PublishState::Private);

				Ok(LintTarget::new(
					workspace_root.to_path_buf(),
					package.manifest_path.clone(),
					contents,
					LintTargetMetadata {
						ecosystem: "npm".to_string(),
						relative_path,
						package_name: Some(package.name),
						package_id,
						group_id,
						managed: configured_package.is_some(),
						private: Some(private),
						publishable: Some(!private),
					},
					Box::new(NpmLintFile {
						manifest,
						workspace_package_names: Arc::clone(&workspace_package_names),
					}),
				))
			})
			.collect()
	}
}

fn is_lintable_workspace_manifest(workspace_root: &Path, manifest_path: &Path) -> bool {
	!(manifest_path.starts_with(workspace_root.join("fixtures"))
		|| manifest_path.starts_with(workspace_root.join("target"))
		|| manifest_path.starts_with(workspace_root.join(".git")))
}

fn configured_package<'a>(
	configuration: &'a WorkspaceConfiguration,
	workspace_root: &Path,
	manifest_dir: &Path,
) -> Option<&'a str> {
	let relative_dir = relative_to_root(workspace_root, manifest_dir)?;
	configuration
		.packages
		.iter()
		.find_map(|package| (package.path == relative_dir).then_some(package.id.as_str()))
}

fn npm_file<'a>(ctx: &'a LintContext<'a>) -> Option<&'a NpmLintFile> {
	ctx.parsed_as::<NpmLintFile>()
}

fn dependency_sections() -> [&'static str; 4] {
	[
		"dependencies",
		"devDependencies",
		"peerDependencies",
		"optionalDependencies",
	]
}

fn location(ctx: &LintContext<'_>) -> LintLocation {
	LintLocation::new(ctx.manifest_path, 1, 1)
}

fn location_for_needle(ctx: &LintContext<'_>, needle: &str) -> LintLocation {
	let Some(start) = ctx.contents.find(needle) else {
		return location(ctx);
	};
	let (line, column) = line_column_for_offset(ctx.contents, start).unwrap_or((1, 1));
	LintLocation::new(ctx.manifest_path, line, column).with_span(start, start + needle.len())
}

fn line_column_for_offset(contents: &str, offset: usize) -> Option<(usize, usize)> {
	if offset > contents.len() || !contents.is_char_boundary(offset) {
		return None;
	}
	let mut line = 1usize;
	let mut column = 1usize;
	for character in contents[..offset].chars() {
		if character == '\n' {
			line += 1;
			column = 1;
		} else {
			column += 1;
		}
	}
	Some((line, column))
}

fn manifest_object_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
	value.as_object_mut()
}

fn dependency_section_object_span(contents: &str, section: &str) -> Option<(usize, usize)> {
	let section_anchor = format!("\"{section}\"");
	let section_start = contents.find(&section_anchor)?;
	let rest = &contents[section_start..];
	let open_offset = rest.find('{')? + section_start;
	let close_offset = matching_brace_offset(contents, open_offset)?;
	Some((open_offset, close_offset + 1))
}

fn source_key_order(contents: &str, section: &str, keys: &[&String]) -> Option<Vec<String>> {
	let (open_offset, close_offset) = dependency_section_object_span(contents, section)?;
	let section_text = &contents[open_offset..close_offset];
	let mut keyed_positions = keys
		.iter()
		.filter_map(|key| {
			section_text
				.find(&format!("\"{key}\""))
				.map(|position| ((*key).clone(), position))
		})
		.collect::<Vec<_>>();
	keyed_positions.sort_by_key(|(_, position)| *position);
	Some(
		keyed_positions
			.into_iter()
			.map(|(key, _)| key)
			.collect::<Vec<_>>(),
	)
}

fn dependency_value_span(
	contents: &str,
	section: &str,
	dep_name: &str,
	version: &str,
) -> Option<(usize, usize)> {
	let (open_offset, close_offset) = dependency_section_object_span(contents, section)?;
	let section_text = &contents[open_offset..close_offset];
	let dep_start = section_text.find(&serde_json::to_string(dep_name).ok()?)?;
	let after_dep = open_offset + dep_start;
	let value_text = serde_json::to_string(version).ok()?;
	let value_start = contents[after_dep..close_offset].find(&value_text)? + after_dep;
	Some((value_start, value_start + value_text.len()))
}

fn leading_line_indent(contents: &str, offset: usize) -> &str {
	let line_start = contents[..offset].rfind('\n').map_or(0, |index| index + 1);
	let indent_end = contents[line_start..]
		.find(|character: char| !character.is_whitespace() || character == '\n')
		.map_or(offset, |index| line_start + index);
	&contents[line_start..indent_end]
}

fn sorted_dependency_section_text(
	contents: &str,
	section: &str,
	object: &Map<String, Value>,
	sorted_keys: &[String],
) -> Option<String> {
	let (open_offset, close_offset) = dependency_section_object_span(contents, section)?;
	let section_indent = leading_line_indent(contents, open_offset);
	let entry_indent = contents[open_offset + 1..close_offset - 1]
		.find('"')
		.map_or_else(
			|| format!("{section_indent}  "),
			|relative| leading_line_indent(contents, open_offset + 1 + relative).to_string(),
		);
	let mut output = String::from("{");
	for (index, key) in sorted_keys.iter().enumerate() {
		let value = object.get(key)?;
		let key_text = serde_json::to_string(key).ok()?;
		let value_text = serde_json::to_string(value).ok()?;
		let comma = if index + 1 == sorted_keys.len() {
			""
		} else {
			","
		};
		write!(output, "\n{entry_indent}{key_text}: {value_text}{comma}").ok()?;
	}
	write!(output, "\n{section_indent}}}").ok()?;
	Some(output)
}

fn matching_brace_offset(contents: &str, open_offset: usize) -> Option<usize> {
	let mut depth = 0usize;
	for (offset, ch) in contents[open_offset..].char_indices() {
		match ch {
			'{' => depth += 1,
			'}' => {
				depth -= 1;
				if depth == 0 {
					return Some(open_offset + offset);
				}
			}
			_ => {}
		}
	}
	None
}

#[derive(Debug)]
struct WorkspaceProtocolRule {
	rule: LintRule,
}

impl WorkspaceProtocolRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/workspace-protocol",
				"Workspace protocol",
				"Requires internal npm-family dependencies to use the workspace: protocol",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![
				LintOptionDefinition::new(
					"require_for_private",
					"also enforce the rule for private packages",
					LintOptionKind::Boolean,
				),
				LintOptionDefinition::new(
					"fix",
					"apply an autofix that rewrites the dependency value",
					LintOptionKind::Boolean,
				),
			]),
		}
	}
}

impl LintRuleRunner for WorkspaceProtocolRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if ctx.metadata.private == Some(true) && !config.bool_option("require_for_private", false) {
			return Vec::new();
		}
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in dependency_sections() {
			let Some(object) = file.manifest.get(section).and_then(Value::as_object) else {
				continue;
			};
			for (dep_name, version) in object {
				let Some(version) = version.as_str() else {
					continue;
				};
				if !file.workspace_package_names.contains(dep_name)
					|| version.starts_with("workspace:")
				{
					continue;
				}

				let mut result = LintResult::new(
					self.rule.id.clone(),
					location_for_needle(ctx, dep_name),
					format!(
						"internal dependency `{dep_name}` should use the workspace: protocol (found `{version}`)"
					),
					config.severity(),
				);

				if config.bool_option("fix", true)
					&& let Some(span) =
						dependency_value_span(ctx.contents, section, dep_name, version)
				{
					result = result.with_fix(LintFix::single(
						"rewrite dependency to workspace:*",
						span,
						"\"workspace:*\"".to_string(),
					));
				}

				results.push(result);
			}
		}

		results
	}
}

#[derive(Debug)]
struct SortedDependenciesRule {
	rule: LintRule,
}

impl SortedDependenciesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/sorted-dependencies",
				"Sorted dependencies",
				"Requires npm-family dependency sections to be alphabetically sorted",
				LintCategory::Style,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that rewrites the manifest with sorted dependency sections",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for SortedDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in dependency_sections() {
			let Some(object) = file.manifest.get(section).and_then(Value::as_object) else {
				continue;
			};
			let keys = object.keys().collect::<Vec<_>>();
			let source_order = source_key_order(ctx.contents, section, &keys)
				.unwrap_or_else(|| keys.iter().map(|key| (*key).clone()).collect::<Vec<_>>());
			let mut sorted_keys = keys.iter().map(|key| (*key).clone()).collect::<Vec<_>>();
			sorted_keys.sort();
			if source_order == sorted_keys {
				continue;
			}

			let mut result = LintResult::new(
				self.rule.id.clone(),
				location_for_needle(ctx, section),
				format!("dependencies in `{section}` are not sorted alphabetically"),
				config.severity(),
			);
			if config.bool_option("fix", true)
				&& let Some(span) = dependency_section_object_span(ctx.contents, section)
				&& let Some(replacement) =
					sorted_dependency_section_text(ctx.contents, section, object, &sorted_keys)
			{
				result = result.with_fix(LintFix::single(
					"sort dependency section alphabetically",
					span,
					replacement,
				));
			}
			results.push(result);
		}

		results
	}
}

#[derive(Debug)]
struct RequiredPackageFieldsRule {
	rule: LintRule,
}

impl RequiredPackageFieldsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/required-package-fields",
				"Required package fields",
				"Requires selected package.json fields to be present",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fields",
				"list of package.json fields that must be present",
				LintOptionKind::StringList,
			)]),
		}
	}
}

impl LintRuleRunner for RequiredPackageFieldsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		config
			.string_list_option("fields")
			.unwrap_or_else(|| {
				vec![
					"description".to_string(),
					"repository".to_string(),
					"license".to_string(),
				]
			})
			.into_iter()
			.filter(|field| file.manifest.get(field).is_none())
			.map(|field| {
				LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!("missing required package.json field `{field}`"),
					config.severity(),
				)
			})
			.collect()
	}
}

#[derive(Debug)]
struct RootNoProdDepsRule {
	rule: LintRule,
}

impl RootNoProdDepsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/root-no-prod-deps",
				"Root no production dependencies",
				"Requires the root package.json to keep production dependencies out of dependencies",
				LintCategory::BestPractice,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that moves dependencies into devDependencies",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for RootNoProdDepsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if ctx.manifest_path.parent() != Some(ctx.workspace_root) {
			return Vec::new();
		}
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let Some(deps) = file.manifest.get("dependencies").and_then(Value::as_object) else {
			return Vec::new();
		};
		if deps.is_empty() {
			return Vec::new();
		}

		let mut result = LintResult::new(
			self.rule.id.clone(),
			location_for_needle(ctx, "dependencies"),
			"root package.json should not have production dependencies; move them to devDependencies",
			config.severity(),
		);
		if config.bool_option("fix", true) {
			let mut rewritten = file.manifest.clone();
			if let Some(root) = manifest_object_mut(&mut rewritten) {
				let moved = root
					.remove("dependencies")
					.and_then(|value| value.as_object().cloned())
					.unwrap_or_default();
				let dev_dependencies = root
					.entry("devDependencies".to_string())
					.or_insert_with(|| Value::Object(Map::new()));
				if let Some(dev_dependencies) = dev_dependencies.as_object_mut() {
					for (name, value) in moved {
						dev_dependencies.insert(name, value);
					}
				}
			}
			result = result.with_fix(LintFix::single(
				"move root dependencies to devDependencies",
				(0, ctx.contents.len()),
				serde_json::to_string_pretty(&rewritten)
					.unwrap_or_else(|_| ctx.contents.to_string()),
			));
		}
		vec![result]
	}
}

#[derive(Debug)]
struct NoDuplicateDependenciesRule {
	rule: LintRule,
}

impl NoDuplicateDependenciesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/no-duplicate-dependencies",
				"No duplicate dependencies",
				"Prevents one dependency from appearing in multiple npm-family dependency sections",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that removes duplicate entries from later sections",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for NoDuplicateDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut seen = BTreeMap::<String, Vec<&'static str>>::new();
		for section in dependency_sections() {
			let Some(object) = file.manifest.get(section).and_then(Value::as_object) else {
				continue;
			};
			for dep_name in object.keys() {
				seen.entry(dep_name.clone()).or_default().push(section);
			}
		}

		let mut results = Vec::new();
		for (dep_name, sections) in seen {
			if sections.len() <= 1 {
				continue;
			}
			let mut result = LintResult::new(
				self.rule.id.clone(),
				location_for_needle(ctx, &dep_name),
				format!(
					"dependency `{dep_name}` appears in multiple sections: {}",
					sections.join(", ")
				),
				config.severity(),
			);
			if config.bool_option("fix", true) {
				let mut rewritten = file.manifest.clone();
				if let Some(root) = manifest_object_mut(&mut rewritten) {
					let keep_in = if sections.contains(&"devDependencies") {
						"devDependencies"
					} else {
						sections.first().copied().unwrap_or("dependencies")
					};
					for section in &sections {
						if *section == keep_in {
							continue;
						}
						if let Some(section_obj) =
							root.get_mut(*section).and_then(Value::as_object_mut)
						{
							section_obj.remove(&dep_name);
						}
					}
				}
				result = result.with_fix(LintFix::single(
					"remove duplicate dependency entries from later sections",
					(0, ctx.contents.len()),
					serde_json::to_string_pretty(&rewritten)
						.unwrap_or_else(|_| ctx.contents.to_string()),
				));
			}
			results.push(result);
		}
		results
	}
}

#[derive(Debug)]
struct UnlistedPackagePrivateRule {
	rule: LintRule,
}

impl UnlistedPackagePrivateRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/unlisted-package-private",
				"Unlisted package must be private",
				"Requires unmanaged npm-family packages to declare private: true",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that inserts private: true",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for UnlistedPackagePrivateRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if ctx.metadata.managed || ctx.metadata.private == Some(true) {
			return Vec::new();
		}
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut result = LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			"unmanaged npm-family packages must set private: true or be declared in monochange.toml",
			config.severity(),
		);
		if config.bool_option("fix", true) {
			let mut rewritten = file.manifest.clone();
			if let Some(root) = manifest_object_mut(&mut rewritten) {
				root.insert("private".to_string(), Value::Bool(true));
			}
			result = result.with_fix(LintFix::single(
				"insert private: true",
				(0, ctx.contents.len()),
				serde_json::to_string_pretty(&rewritten)
					.unwrap_or_else(|_| ctx.contents.to_string()),
			));
		}
		vec![result]
	}
}

#[cfg(test)]
#[path = "__tests__/mod_tests.rs"]
mod tests;
