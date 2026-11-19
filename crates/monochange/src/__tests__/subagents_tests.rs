#![allow(clippy::disallowed_methods)]
use std::path::Path;

use monochange_test_helpers::fs::fixture_path_from;
use monochange_test_helpers::fs::setup_scenario_workspace_from;

use super::*;

fn fixture_readme_contents() -> String {
	fs::read_to_string(fixture_path_from(
		env!("CARGO_MANIFEST_DIR"),
		"subagents/basic/workspace/README.md",
	))
	.unwrap_or_else(|error| panic!("read fixture README: {error}"))
}

#[test]
fn write_subagent_plan_reports_conflicts_and_io_failures() {
	let workspace = setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), "subagents/basic");
	let contents = fixture_readme_contents();
	let plan = SubagentPlan {
		targets: vec![SubagentTarget::Claude],
		files: vec![
			GeneratedFile {
				path: PathBuf::from("README.md"),
				description: "README".to_string(),
				operation: GeneratedFileOperation::Overwrite,
				contents: contents.clone(),
			},
			GeneratedFile {
				path: PathBuf::from(".mcp.json"),
				description: "MCP".to_string(),
				operation: GeneratedFileOperation::Overwrite,
				contents: contents.clone(),
			},
		],
		notes: Vec::new(),
		dry_run: false,
	};
	let error = write_subagent_plan(workspace.path(), &plan, false)
		.err()
		.unwrap_or_else(|| panic!("expected overwrite conflict"));
	assert_eq!(
		error.to_string(),
		"config error: refusing to overwrite existing subagent files without --force: README.md, .mcp.json"
	);

	let parent_file_path = workspace.path().join("parent-file");
	fs::write(&parent_file_path, &contents)
		.unwrap_or_else(|error| panic!("write parent-file fixture: {error}"));
	let parent_error_plan = SubagentPlan {
		targets: vec![SubagentTarget::Pi],
		files: vec![GeneratedFile {
			path: PathBuf::from("parent-file/child.md"),
			description: "nested child".to_string(),
			operation: GeneratedFileOperation::Create,
			contents: contents.clone(),
		}],
		notes: Vec::new(),
		dry_run: false,
	};
	let parent_error = write_subagent_plan(workspace.path(), &parent_error_plan, true)
		.err()
		.unwrap_or_else(|| panic!("expected create_dir_all failure"));
	assert!(
		parent_error
			.to_string()
			.contains(&format!("failed to create {}", parent_file_path.display()))
	);

	let directory_path = workspace.path().join("existing-directory");
	fs::create_dir_all(&directory_path)
		.unwrap_or_else(|error| panic!("create existing-directory fixture: {error}"));
	let write_error_plan = SubagentPlan {
		targets: vec![SubagentTarget::Pi],
		files: vec![GeneratedFile {
			path: PathBuf::from("existing-directory"),
			description: "directory write error".to_string(),
			operation: GeneratedFileOperation::Create,
			contents,
		}],
		notes: Vec::new(),
		dry_run: false,
	};
	let write_error = write_subagent_plan(workspace.path(), &write_error_plan, true)
		.err()
		.unwrap_or_else(|| panic!("expected file write failure"));
	assert!(
		write_error
			.to_string()
			.contains(&format!("failed to write {}", directory_path.display()))
	);

	let no_parent_plan = SubagentPlan {
		targets: vec![SubagentTarget::Cursor],
		files: vec![GeneratedFile {
			path: PathBuf::new(),
			description: "no parent path".to_string(),
			operation: GeneratedFileOperation::Create,
			contents: String::new(),
		}],
		notes: Vec::new(),
		dry_run: false,
	};
	write_subagent_plan(Path::new(""), &no_parent_plan, true)
		.unwrap_or_else(|error| panic!("write empty path plan: {error}"));
}
