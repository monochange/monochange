use std::cmp::Ordering;

use toml_edit::Item;
use toml_edit::Table;

use super::*;

const UNSORTED_CONFIGURATION: &str = r#"#:schema ./schema.json

# =============================================================================
# project configuration
# =============================================================================

[cli.release]
help_text = "Release packages"

[group.zeta]
packages = ["zeta-tool", "alpha-tool"]

# The package comment moves with its table.
[package.zeta]
path = "packages/zeta"

[defaults]
parent_bump = "patch"

[package.alpha]
path = "packages/alpha"

[group.alpha]
packages = [
	"delta-tool",
	"charlie-tool",
]

[changelog]
templates = ["- {{ summary }}"]

[version_scheme.calver]
template = "{{ year }}.{{ month }}"
"#;

#[test]
fn formats_sections_packages_groups_and_group_members() {
	insta::assert_snapshot!(format_workspace_configuration(UNSORTED_CONFIGURATION).unwrap());
}

#[test]
fn sorts_package_tables_by_id() {
	let source = r#"
[package.monochange]
path = "crates/monochange"

[package.monochange_app]
path = "app"

[package.monochange_core]
path = "crates/monochange_core"

[package.monochange_snapshot]
path = "crates/monochange_snapshot"

[package.monochange_cargo]
path = "crates/monochange_cargo"
"#;
	let formatted = format_workspace_configuration(source).unwrap();
	let order = formatted
		.lines()
		.filter(|line| line.starts_with("[package."))
		.collect::<Vec<_>>();

	assert_eq!(
		order,
		vec![
			"[package.monochange]",
			"[package.monochange_app]",
			"[package.monochange_cargo]",
			"[package.monochange_core]",
			"[package.monochange_snapshot]",
		]
	);
}

#[test]
fn keeps_a_map_section_header_at_the_start_when_children_move() {
	let source = r#"
[defaults]
parent_bump = "patch"

# =============================================================================
# Packages
# =============================================================================
#
# Shared package documentation.

# Zeta-specific comment.
[package.zeta]
path = "packages/zeta"

[package.alpha]
path = "packages/alpha"
"#;

	insta::assert_snapshot!(format_workspace_configuration(source).unwrap());
}

#[test]
fn formatting_is_idempotent() {
	let once = format_workspace_configuration(UNSORTED_CONFIGURATION).unwrap();
	let twice = format_workspace_configuration(&once).unwrap();

	assert_eq!(once, twice);
}

#[test]
fn keeps_unknown_sections_after_known_sections() {
	let source = r#"
[custom]
value = true

[package.web]
path = "packages/web"

[defaults]
parent_bump = "patch"
"#;
	let formatted = format_workspace_configuration(source).unwrap();

	assert!(formatted.find("[defaults]").unwrap() < formatted.find("[package.web]").unwrap());
	assert!(formatted.find("[package.web]").unwrap() < formatted.find("[custom]").unwrap());
}

#[test]
fn leaves_non_string_group_members_in_author_order() {
	let source = r#"
[group.main]
packages = ["zeta", 1]

[package.zeta]
path = "packages/zeta"
values = { zeta = 1, alpha = 2 }

[package.alpha]
path = "packages/alpha"
"#;
	let formatted = format_workspace_configuration(source).unwrap();

	assert!(formatted.contains("packages = [\"zeta\", 1]"));
	assert!(formatted.contains("values = { zeta = 1, alpha = 2 }"));
}

#[test]
fn leaves_an_empty_document_empty() {
	assert_eq!(format_workspace_configuration("").unwrap(), "");
}

#[test]
fn keeps_a_comment_only_document() {
	let formatted = format_workspace_configuration("# only a comment\n").unwrap();

	assert_eq!(formatted, "# only a comment\n\n");
}

#[test]
fn covers_ordering_and_defensive_helpers() {
	assert_eq!(trim_leading_blank_lines("\n\n# comment\n"), "# comment\n");
	assert_eq!(
		compare_keys("custom", "defaults", ROOT_SECTION_ORDER),
		Ordering::Greater
	);
	assert_eq!(
		compare_keys("defaults", "custom", ROOT_SECTION_ORDER),
		Ordering::Less
	);
	assert_eq!(
		compare_keys("alpha", "beta", ROOT_SECTION_ORDER),
		Ordering::Equal
	);

	let mut empty = Table::new();
	assert_eq!(take_section_header(&mut empty), "");

	let mut missing = Table::new();
	attach_section_header(&mut missing, "missing", "header");
	assert!(missing.is_empty());

	let mut empty_parent = Table::new();
	empty_parent.insert("package", Item::Table(Table::new()));
	attach_section_header(&mut empty_parent, "package", "header");

	let mut value_parent = Table::new();
	let mut parent = Table::new();
	parent.insert("note", toml_edit::value("x"));
	value_parent.insert("package", Item::Table(parent));
	attach_section_header(&mut value_parent, "package", "header");
}

#[test]
fn formats_deployments_and_nested_package_tables() {
	let source = r#"
[[deployments]]
name = "docs"
trigger = "release_published"

[defaults]
parent_bump = "patch"

[package.web]
path = "packages/web"

[package.web.publish]
enabled = false

[[package.web.deployments]]
name = "nested"

[[deployments]]
name = "app"
trigger = "workflow"
"#;

	insta::assert_snapshot!(format_workspace_configuration(source).unwrap());
}

#[test]
fn keeps_unknown_sections_in_their_relative_order() {
	let source = r"
[zeta]
value = 1

[alpha]
value = 2
";
	let formatted = format_workspace_configuration(source).unwrap();

	assert!(formatted.find("[zeta]").unwrap() < formatted.find("[alpha]").unwrap());
}

#[test]
fn keeps_child_comments_when_the_first_table_changes() {
	let source = r#"
[defaults]
parent_bump = "patch"

# Packages

# Zeta comment.
[package.zeta]
path = "packages/zeta"

# Alpha comment.
[package.alpha]
path = "packages/alpha"
"#;

	insta::assert_snapshot!(format_workspace_configuration(source).unwrap());
}

#[test]
fn preserves_root_values_before_tables() {
	let source = r#"
title = "workspace"

[package.zeta]
path = "packages/zeta"

[package.alpha]
path = "packages/alpha"
"#;

	let formatted = format_workspace_configuration(source).unwrap();
	eprintln!("ROOT VALUE OUTPUT: {formatted:?}");
	insta::assert_snapshot!(formatted);
}

#[test]
fn ignores_malformed_group_entries() {
	let source = r#"
[group]
note = "not a group table"

[group.empty]
tag = true

[group.main]
packages = ["zeta", "alpha"]

[package.zeta]
path = "packages/zeta"

[package.alpha]
path = "packages/alpha"
"#;

	insta::assert_snapshot!(format_workspace_configuration(source).unwrap());
}

#[test]
fn reports_invalid_toml() {
	let error = format_workspace_configuration("[defaults").unwrap_err();

	assert!(
		error
			.to_string()
			.contains("failed to parse monochange.toml")
	);
}
