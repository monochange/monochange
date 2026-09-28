use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Serialize;
use serde_json::json;

use crate::SubagentOutputFormat;
use crate::SubagentTarget;

#[derive(Debug, Clone)]
pub(crate) struct SubagentOptions {
	pub targets: Vec<SubagentTarget>,
	pub force: bool,
	pub dry_run: bool,
	pub format: SubagentOutputFormat,
	pub generate_mcp: bool,
}

#[derive(Debug, Clone)]
struct GeneratedFileDraft {
	path: PathBuf,
	description: String,
	contents: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GeneratedFileOperation {
	Create,
	Overwrite,
	Skip,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct GeneratedFile {
	pub path: PathBuf,
	pub description: String,
	pub operation: GeneratedFileOperation,
	pub contents: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct SubagentPlan {
	pub targets: Vec<SubagentTarget>,
	pub files: Vec<GeneratedFile>,
	pub notes: Vec<String>,
	pub dry_run: bool,
}

pub(crate) fn run_subagents(root: &Path, options: &SubagentOptions) -> MonochangeResult<String> {
	let plan = build_subagent_plan(root, options)?;

	if !options.dry_run {
		write_subagent_plan(root, &plan, options.force)?;
	}

	match options.format {
		SubagentOutputFormat::Json | SubagentOutputFormat::JsonMin => {
			render_subagent_plan_json(&plan, options.format)
		}
		SubagentOutputFormat::Markdown | SubagentOutputFormat::Text => {
			Ok(render_subagent_plan_text(&plan))
		}
	}
}

fn render_subagent_plan_json(
	plan: &SubagentPlan,
	format: SubagentOutputFormat,
) -> MonochangeResult<String> {
	match format {
		SubagentOutputFormat::JsonMin => {
			serde_json::to_string(&plan).map_err(|error| MonochangeError::Config(error.to_string()))
		}
		_ => {
			serde_json::to_string_pretty(&plan)
				.map_err(|error| MonochangeError::Config(error.to_string()))
		}
	}
}

fn build_subagent_plan(root: &Path, options: &SubagentOptions) -> MonochangeResult<SubagentPlan> {
	let mut drafts = Vec::new();
	let mut notes = Vec::new();

	for target in &options.targets {
		for draft in generate_target_files(*target, options.generate_mcp, &mut notes)? {
			push_generated_file(&mut drafts, draft)?;
		}
	}

	let files = drafts
		.into_iter()
		.map(|draft| {
			let absolute_path = root.join(&draft.path);
			let operation = classify_generated_file(&absolute_path, &draft.contents)?;

			Ok(GeneratedFile {
				path: draft.path,
				description: draft.description,
				operation,
				contents: draft.contents,
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;

	Ok(SubagentPlan {
		targets: options.targets.clone(),
		files,
		notes,
		dry_run: options.dry_run,
	})
}

fn push_generated_file(
	drafts: &mut Vec<GeneratedFileDraft>,
	draft: GeneratedFileDraft,
) -> MonochangeResult<()> {
	let Some(existing) = drafts.iter().find(|existing| existing.path == draft.path) else {
		drafts.push(draft);
		return Ok(());
	};

	if existing.contents == draft.contents {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"multiple subagent targets attempted to generate different contents for {}",
		draft.path.display()
	)))
}

fn classify_generated_file(
	path: &Path,
	expected: &str,
) -> MonochangeResult<GeneratedFileOperation> {
	if !path.exists() {
		return Ok(GeneratedFileOperation::Create);
	}

	let current = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;

	if current == expected {
		return Ok(GeneratedFileOperation::Skip);
	}

	Ok(GeneratedFileOperation::Overwrite)
}

fn write_subagent_plan(root: &Path, plan: &SubagentPlan, force: bool) -> MonochangeResult<()> {
	if !force {
		let mut conflict_message = String::new();
		for file in plan
			.files
			.iter()
			.filter(|file| file.operation == GeneratedFileOperation::Overwrite)
		{
			if conflict_message.is_empty() {
				conflict_message
					.push_str("refusing to overwrite existing subagent files without --force: ");
			} else {
				conflict_message.push_str(", ");
			}
			let _ = write!(conflict_message, "{}", file.path.display());
		}
		if !conflict_message.is_empty() {
			return Err(MonochangeError::Config(conflict_message));
		}
	}

	for file in &plan.files {
		if file.operation == GeneratedFileOperation::Skip {
			continue;
		}

		let absolute_path = root.join(&file.path);
		let Some(parent) = absolute_path.parent() else {
			continue;
		};

		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
		fs::write(&absolute_path, &file.contents).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to write {}: {error}",
				absolute_path.display()
			))
		})?;
	}

	Ok(())
}

fn render_subagent_plan_text(plan: &SubagentPlan) -> String {
	let mut output = String::new();
	let _ = writeln!(output, "monochange subagents");
	let _ = writeln!(output);
	let _ = writeln!(output, "Targets:");

	for target in &plan.targets {
		let _ = writeln!(output, "- {}", subagent_target_name(*target));
	}

	let _ = writeln!(output);
	let _ = writeln!(output, "Files:");

	for file in &plan.files {
		let _ = writeln!(
			output,
			"- {} {}",
			generated_file_operation_name(&file.operation),
			file.path.display()
		);
	}

	if !plan.notes.is_empty() {
		let _ = writeln!(output);
		let _ = writeln!(output, "Notes:");

		for note in &plan.notes {
			let _ = writeln!(output, "- {note}");
		}
	}

	if plan.dry_run {
		let _ = writeln!(output);
		let _ = writeln!(output, "Dry run only. No files were written.");
	}

	output.trim_end().to_string()
}

fn generated_file_operation_name(operation: &GeneratedFileOperation) -> &'static str {
	match operation {
		GeneratedFileOperation::Create => "create",
		GeneratedFileOperation::Overwrite => "overwrite",
		GeneratedFileOperation::Skip => "skip",
	}
}

fn generate_target_files(
	target: SubagentTarget,
	generate_mcp: bool,
	notes: &mut Vec<String>,
) -> MonochangeResult<Vec<GeneratedFileDraft>> {
	let mut files = Vec::new();

	match target {
		SubagentTarget::Claude => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".claude/agents/monochange-release-agent.md"),
				description: "Claude subagent definition".to_string(),
				contents: render_claude_agent(),
			});

			if generate_mcp {
				files.push(GeneratedFileDraft {
					path: PathBuf::from(".mcp.json"),
					description: "Claude MCP server configuration".to_string(),
					contents: render_claude_mcp_config()?,
				});
			}
		}

		SubagentTarget::Vscode | SubagentTarget::Copilot => {
			let description = if target == SubagentTarget::Vscode {
				"VS Code"
			} else {
				"GitHub Copilot"
			};
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".github/agents/monochange-release-agent.agent.md"),
				description: format!("{description} agent definition"),
				contents: render_vscode_agent(),
			});

			if generate_mcp {
				files.push(GeneratedFileDraft {
					path: PathBuf::from(".vscode/mcp.json"),
					description: "VS Code MCP server configuration".to_string(),
					contents: render_vscode_mcp_config()?,
				});
			}
		}

		SubagentTarget::Pi => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".pi/agents/monochange-release-agent.md"),
				description: "Pi project agent definition".to_string(),
				contents: render_pi_agent(),
			});
		}

		SubagentTarget::Codex => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".codex/agents/monochange-release-agent.toml"),
				description: "Codex custom agent definition".to_string(),
				contents: render_codex_agent(),
			});
		}

		SubagentTarget::Cursor => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".cursor/rules/monochange.mdc"),
				description: "Cursor workspace rule".to_string(),
				contents: render_cursor_rule(),
			});
			notes.push(
				"Cursor generation currently emits a repo-local workspace rule instead of a native custom subagent manifest.".to_string(),
			);
		}
	}

	Ok(files)
}

fn render_claude_agent() -> String {
	format!(
		"---\nname: monochange-release-agent\ndescription: Use this agent for monochange configuration, changesets, diagnostics, and release planning.\ntools: Bash, Read, Grep, Glob, LS\ncolor: blue\n---\n\n{}\n",
		shared_subagent_instructions(),
	)
}

fn render_vscode_agent() -> String {
	format!(
		"---\nname: monochange-release-agent\ndescription: Use this agent for monochange configuration, changesets, diagnostics, and release planning.\n---\n\n{}\n",
		shared_subagent_instructions(),
	)
}

fn render_pi_agent() -> String {
	format!(
		"---\nname: monochange-release-agent\ndescription: Use this agent for monochange configuration, changesets, diagnostics, and release planning.\ntools: read, grep, find, bash\n---\n\n{}\n",
		shared_subagent_instructions(),
	)
}

fn render_codex_agent() -> String {
	format!(
		"name = \"monochange-release-agent\"\ndescription = \"Use this agent for monochange configuration, changesets, diagnostics, and release planning.\"\ndeveloper_instructions = \"\"\"\n{}\n\"\"\"\n",
		shared_subagent_instructions(),
	)
}

fn render_cursor_rule() -> String {
	format!(
		"---\ndescription: monochange workflow guidance for release planning, changesets, diagnostics, and changelog updates\nglobs:\n  - \"**/*\"\nalwaysApply: false\n---\n\n{}\n",
		shared_cursor_instructions(),
	)
}

fn render_claude_mcp_config() -> MonochangeResult<String> {
	serde_json::to_string_pretty(&json!({
		"mcpServers": {
			"monochange": {
				"command": "npx",
				"args": ["-y", "@monochange/cli", "mcp"]
			}
		}
	}))
	.map_err(|error| MonochangeError::Config(error.to_string()))
}

fn render_vscode_mcp_config() -> MonochangeResult<String> {
	serde_json::to_string_pretty(&json!({
		"servers": {
			"monochange": {
				"type": "stdio",
				"command": "npx",
				"args": ["-y", "@monochange/cli", "mcp"]
			}
		},
		"inputs": []
	}))
	.map_err(|error| MonochangeError::Config(error.to_string()))
}

fn shared_subagent_instructions() -> &'static str {
	"You are the monochange release agent for this repository.

Read the bundled monochange skill before anything else. It ships inside the CLI, so it works offline without npm or a network:
- `monochange skill` lists every bundled topic.
- `monochange skill read monochange` prints the entrypoint: task routing, source-of-truth rules, and what each topic covers.
- Read the focused topics as needed: `monochange skill read commands`, `monochange skill read changesets`, `monochange skill read change-classification`, `monochange skill read configuration`, and `monochange skill read multi-package-publishing`.
- `monochange skill install --dir <dir>` writes the whole skill into an agent runtime's skill directory, for example `monochange skill install --dir .claude/skills/monochange`.

When working on release planning, versioning, changesets, changelogs, or monochange configuration:

1. Read `monochange.toml` before suggesting workflow or release changes. It defines this repository's package ids, groups, ecosystems, and `[cli.*]` workflows.
2. Prefer the monochange CLI over MCP tools.
3. Choose the CLI executable in this order:
   - `monochange`
   - `npx -y @monochange/cli`
4. Choose the command spelling in this order:
   - the repository's configured workflow, `monochange run <name>`, when `monochange run --help` (or `monochange help run`) lists it
   - the short built-in command when one exists, such as `monochange create`, `monochange discover`, `monochange config`, `monochange preview`, `monochange prepare`, `monochange affected`, `monochange diagnose`, `monochange next`, `monochange versions list`, `monochange versions sync`, or `monochange publish packages|readiness|placeholder`
   - `monochange step <name>`, which works in every repository and is the portable form that `[cli.*]` step types bind to
5. Inspect workspace state in the selected format, using JSON when the command supports it:
   - `<cli> step validate`
   - `<cli> discover --format json` (or `<cli> step discover --format json`)
   - `<cli> diagnose --format json` (or `<cli> step diagnose-changesets --format json`)
   - `<cli> change classify --format json --dependency-propagation public`
   - `<cli> preview --format json --diff` (or `<cli> step prepare-release --dry-run --format json`)
6. Before writing release intent, account for every package in the classification report. Use `decision.proposed_changeset_bump` for the current change, trace its `decision.finding_ids`, and treat `review_required: true` as a review task. Read each package's `existing_changesets` and `action` before creating another `.changeset/*.md` file.
7. Run `<cli> changeset validate --api`, `<cli> check`, and `<cli> step validate` after editing release intent.
8. Do not run mutating release or publish commands unless the user explicitly asks.
9. Gitignore only `.monochange/local/`. Never add `.monochange/` as a whole to `.gitignore`: release records (`.monochange/releases/<id>/release.json`) and prerelease state are committed release state that publish, tag, and readiness steps read from git history, so ignoring them makes releases unpublishable.
10. If monochange MCP tools are available, use `monochange_classify_changes` as a secondary structured fallback when the CLI is unavailable.

Recommended workflow:
- validate configuration and changesets: `monochange step validate`
- discover packages: `monochange discover --format json`
- classify change severity: `monochange change classify --format json --dependency-propagation public`
- inspect findings and existing changesets: `monochange diagnose --format json` (requires at least one pending changeset)
- create or update release intent: `monochange create`
- validate the changeset severity: `monochange changeset validate --api`
- preview without writing: `monochange preview --format json --diff`
- report the planned versions: `monochange next`
- summarize the impact and next steps"
}

fn shared_cursor_instructions() -> &'static str {
	"When working in this repository on release planning, versioning, changesets, changelogs, or `monochange.toml`:

Read the bundled monochange skill before anything else. It ships inside the CLI, so it works offline without npm or a network:
- `monochange skill` lists every bundled topic.
- `monochange skill read monochange` prints the entrypoint, and `monochange skill read commands`, `monochange skill read changesets`, `monochange skill read change-classification`, and `monochange skill read configuration` cover the common follow-ups.
- `monochange skill install --dir <dir>` writes the whole skill into an agent runtime's skill directory.

1. Read `monochange.toml` first. It defines this repository's package ids, groups, ecosystems, and `[cli.*]` workflows.
2. Prefer the monochange CLI over MCP tools.
3. Choose the CLI executable in this order:
   - `monochange`
   - `npx -y @monochange/cli`
4. Choose the command spelling in this order:
   - the repository's configured workflow, `monochange run <name>`, when `monochange run --help` (or `monochange help run`) lists it
   - the short built-in command when one exists, such as `monochange create`, `monochange discover`, `monochange preview`, `monochange prepare`, `monochange diagnose`, `monochange next`, `monochange versions sync`, or `monochange publish readiness`
   - `monochange step <name>`, which works in every repository and is the portable form that `[cli.*]` step types bind to
5. Use JSON output when inspecting repository state:
   - `<cli> step validate`
   - `<cli> discover --format json`
   - `<cli> diagnose --format json`
   - `<cli> change classify --format json --dependency-propagation public`
   - `<cli> preview --format json --diff`
6. Use `decision.proposed_changeset_bump` for the current change. Trace `decision.finding_ids` and review every package with `review_required: true`. Read `existing_changesets` and each package's `action` before creating another `.changeset/*.md` file.
7. Run `<cli> changeset validate --api` and `<cli> check` after editing release intent.
8. Do not run mutating release or publish commands unless the user explicitly asks.
9. Gitignore only `.monochange/local/`. Never add `.monochange/` as a whole to `.gitignore`: release records (`.monochange/releases/<id>/release.json`) and prerelease state are committed release state that publish, tag, and readiness steps read from git history, so ignoring them makes releases unpublishable.
10. If monochange MCP tools are configured, use `monochange_classify_changes` as a secondary structured fallback."
}

fn subagent_target_name(target: SubagentTarget) -> &'static str {
	match target {
		SubagentTarget::Claude => "claude",
		SubagentTarget::Vscode => "vscode",
		SubagentTarget::Copilot => "copilot",
		SubagentTarget::Pi => "pi",
		SubagentTarget::Codex => "codex",
		SubagentTarget::Cursor => "cursor",
	}
}

#[cfg(test)]
#[path = "__tests__/subagents_tests.rs"]
mod tests;
