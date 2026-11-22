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
fn formatting_is_idempotent() {
	let once = format_workspace_configuration(UNSORTED_CONFIGURATION).unwrap();
	let twice = format_workspace_configuration(&once).unwrap();

	assert_eq!(once, twice);
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
