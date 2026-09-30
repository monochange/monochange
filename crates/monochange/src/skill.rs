//! Serve the bundled agent skill from the CLI.
//!
//! The skill ships inside the binary so an agent that has only the toolkit — no
//! npm package, no network, no cloned repository — can still read the same
//! guidance a skill installation would give it. `read` writes raw Markdown to
//! stdout: agents parse the file itself, so there is deliberately no terminal
//! rendering on this path.
//!
//! The embedded bytes are a committed copy of `packages/monochange__skill`
//! kept in step by `scripts/docs/sync-skill.mjs`; `docs:check` fails when the
//! copy drifts.
//!
//! # Topic and install-path scheme
//!
//! Skill modules keep their file stem as the topic name (`configuration` for
//! `skills/configuration.md`). Examples are prefixed with `example-` because
//! `skills/readme.md` and `examples/readme.md` share a stem, so unprefixed
//! example topics would collide with module topics. Install paths always mirror
//! the package layout (`SKILL.md`, `skills/<stem>.md`, `examples/<stem>.md`) so
//! the relative links written in the documents keep resolving after install.

use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

/// The entrypoint document agents load first.
const SKILL_ENTRY: &str = include_str!("../skill/SKILL.md");

/// A skill document that can be listed, read by topic, or installed.
struct Topic {
	/// Name accepted on the command line.
	name: &'static str,
	/// Path the document carries in an installation, slash-separated.
	file: &'static str,
	/// Description shown by the listing.
	description: &'static str,
	content: &'static str,
}

/// Every bundled document, entrypoint first.
const TOPICS: &[Topic] = &[
	Topic {
		name: "monochange",
		file: "SKILL.md",
		description: "entrypoint: routing, invariants, and where each task is covered",
		content: SKILL_ENTRY,
	},
	Topic {
		name: "adoption",
		file: "skills/adoption.md",
		description: "adoption checklist for migrating an existing release workflow",
		content: include_str!("../skill/skills/adoption.md"),
	},
	Topic {
		name: "artifact-types",
		file: "skills/artifact-types.md",
		description: "release-note artifact types and changeset wording",
		content: include_str!("../skill/skills/artifact-types.md"),
	},
	Topic {
		name: "change-classification",
		file: "skills/change-classification.md",
		description: "choose changeset severity from compatibility evidence",
		content: include_str!("../skill/skills/change-classification.md"),
	},
	Topic {
		name: "changeset-guide",
		file: "skills/changeset-guide.md",
		description: "changeset wording guide with examples",
		content: include_str!("../skill/skills/changeset-guide.md"),
	},
	Topic {
		name: "changesets",
		file: "skills/changesets.md",
		description: "author, validate, and diagnose .changeset release intent",
		content: include_str!("../skill/skills/changesets.md"),
	},
	Topic {
		name: "commands",
		file: "skills/commands.md",
		description: "verified command inventory and [cli.*] step composition",
		content: include_str!("../skill/skills/commands.md"),
	},
	Topic {
		name: "configuration",
		file: "skills/configuration.md",
		description: "monochange.toml structure, package ids, ecosystems, and workflows",
		content: include_str!("../skill/skills/configuration.md"),
	},
	Topic {
		name: "linting",
		file: "skills/linting.md",
		description: "monochange check, lint presets, and manifest policy",
		content: include_str!("../skill/skills/linting.md"),
	},
	Topic {
		name: "multi-package-publishing",
		file: "skills/multi-package-publishing.md",
		description: "publish readiness, bootstrap, plans, and package publishing",
		content: include_str!("../skill/skills/multi-package-publishing.md"),
	},
	Topic {
		name: "readme",
		file: "skills/readme.md",
		description: "index of every focused skill module",
		content: include_str!("../skill/skills/readme.md"),
	},
	Topic {
		name: "reference",
		file: "skills/reference.md",
		description: "full end-to-end operating reference",
		content: include_str!("../skill/skills/reference.md"),
	},
	Topic {
		name: "trusted-publishing",
		file: "skills/trusted-publishing.md",
		description: "OIDC and trusted-publishing setup for registries",
		content: include_str!("../skill/skills/trusted-publishing.md"),
	},
	Topic {
		name: "example-migration",
		file: "examples/migration.md",
		description: "example: migrate a mixed Cargo and npm repository",
		content: include_str!("../skill/examples/migration.md"),
	},
	Topic {
		name: "example-publishing",
		file: "examples/publishing.md",
		description: "example: readiness, bootstrap, and publishing workflow",
		content: include_str!("../skill/examples/publishing.md"),
	},
	Topic {
		name: "example-quickstart",
		file: "examples/quickstart.md",
		description: "example: minimal npm workspace config",
		content: include_str!("../skill/examples/quickstart.md"),
	},
	Topic {
		name: "example-readme",
		file: "examples/readme.md",
		description: "example: index of copyable scenarios",
		content: include_str!("../skill/examples/readme.md"),
	},
	Topic {
		name: "example-release-pr",
		file: "examples/release-pr.md",
		description: "example: release pull request workflow",
		content: include_str!("../skill/examples/release-pr.md"),
	},
];

/// A `monochange skill` operation.
pub(crate) enum SkillAction {
	/// Describe the bundled topics.
	List,
	/// Print one document verbatim.
	Read {
		/// Topic name, or `None` for the listing.
		topic: Option<String>,
	},
	/// Write the whole skill into a directory.
	Install {
		/// Target skill directory, or `None` to fail with usage help.
		destination: Option<PathBuf>,
		/// Replace an existing skill when one is present.
		force: bool,
	},
}

/// What a skill operation produced.
pub(crate) enum SkillOutcome {
	/// Listing text for the caller to print.
	Listed(String),
	/// A document to write to stdout byte for byte.
	Read {
		/// The document contents.
		content: &'static str,
	},
	/// The tree was written to disk.
	Installed {
		/// Directory the tree was written under.
		destination: PathBuf,
		/// Number of documents written.
		files: usize,
	},
}

/// Every topic the CLI can serve.
fn topics() -> &'static [Topic] {
	TOPICS
}

/// Run a `monochange skill` operation.
pub(crate) fn run_skill(action: SkillAction) -> MonochangeResult<SkillOutcome> {
	match action {
		SkillAction::List => Ok(SkillOutcome::Listed(listing())),
		SkillAction::Read { topic } => read(topic.as_deref()),
		SkillAction::Install { destination, force } => install(destination, force),
	}
}

/// Write an outcome for the CLI: raw bytes for `read`, a summary otherwise.
pub(crate) fn render_outcome(outcome: SkillOutcome) -> MonochangeResult<String> {
	match outcome {
		SkillOutcome::Listed(listing) => Ok(listing),
		SkillOutcome::Read { content } => {
			write_verbatim(content)?;
			Ok(String::new())
		}
		SkillOutcome::Installed { destination, files } => {
			let plural = if files == 1 { "file" } else { "files" };
			Ok(format!(
				"Installed {files} {plural} into {}\nPoint the agent runtime at that directory as a skill named `monochange`.",
				destination.display()
			))
		}
	}
}

/// Render the topic listing with install paths and usage examples.
fn listing() -> String {
	let topic_width = topics()
		.iter()
		.map(|topic| topic.name.len())
		.max()
		.unwrap_or_default()
		+ 2;
	let file_width = topics()
		.iter()
		.map(|topic| topic.file.len())
		.max()
		.unwrap_or_default()
		+ 2;
	let mut output =
		String::from("The monochange agent skill is bundled in this binary. Read a topic:\n");
	for topic in topics() {
		// Writing to a `String` cannot fail.
		let _ = writeln!(
			output,
			"  {:<topic_width$}{:<file_width$}{}",
			topic.name, topic.file, topic.description
		);
	}
	output.push('\n');
	output.push_str("  monochange skill read <topic>            # raw Markdown on stdout\n");
	output.push_str(
		"  monochange skill install --dir <dir>     # write the whole skill for an agent runtime\n",
	);
	output
}

fn read(topic: Option<&str>) -> MonochangeResult<SkillOutcome> {
	let Some(topic) = topic else {
		return Ok(SkillOutcome::Listed(listing()));
	};

	let Some(matched) = topics().iter().find(|candidate| candidate.name == topic) else {
		let available = topics()
			.iter()
			.map(|candidate| candidate.name)
			.collect::<Vec<_>>()
			.join(", ");
		return Err(MonochangeError::Config(format!(
			"unknown skill topic `{topic}`; available topics: {available}"
		)));
	};

	Ok(SkillOutcome::Read {
		content: matched.content,
	})
}

fn install(destination: Option<PathBuf>, force: bool) -> MonochangeResult<SkillOutcome> {
	let Some(destination) = destination else {
		return Err(MonochangeError::Config(missing_destination_message()));
	};
	validate_install_paths(&destination)?;

	if destination.join("SKILL.md").exists() && !force {
		return Err(MonochangeError::Config(format!(
			"{} already contains a skill; pass --force to replace it",
			destination.display()
		)));
	}
	// `write_tree` names the exact file that failed; the destination is added
	// here so an agent knows which install directory was being populated.
	let files = write_tree(&destination).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write the skill into {}: {error}",
			destination.display()
		))
	})?;
	Ok(SkillOutcome::Installed { destination, files })
}

/// Reject linked skill paths before writes can change operator-owned files.
fn validate_install_paths(directory: &Path) -> MonochangeResult<()> {
	for topic in topics() {
		let path = directory.join(topic.file);

		// Security: static relative paths still escape through existing symlinks.
		// Only inspect the selected tree; runtime directories above it may be linked.
		for component in path
			.ancestors()
			.take_while(|component| component.starts_with(directory))
		{
			match std::fs::symlink_metadata(component) {
				Ok(metadata) if metadata.is_symlink() => {
					return Err(MonochangeError::Config(format!(
						"refusing to install a skill through symbolic link {}",
						component.display()
					)));
				}
				Ok(_) => {}
				Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
				Err(error) => {
					return Err(MonochangeError::Io(format!(
						"failed to inspect skill path {}: {error}",
						component.display()
					)));
				}
			}
		}
	}

	Ok(())
}

/// Write every bundled document under `directory`, returning the file count.
fn write_tree(directory: &Path) -> MonochangeResult<usize> {
	let mut written = 0usize;
	for topic in topics() {
		// Static install paths have been checked for existing symlinks before writing.
		let path = directory.join(topic.file);
		// patch-coverage:ignore-start -- the create and write failures are exercised
		// by the install failure tests; llvm-cov attributes a zero-count region to
		// the `if let` fall-through that no execution can reach.
		if let Some(parent) = path.parent() {
			let created = std::fs::create_dir_all(parent).map_err(|error| {
				MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
			});
			created?;
		}
		// patch-coverage:ignore-end
		std::fs::write(&path, topic.content).map_err(|error| {
			MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
		})?;
		written += 1;
	}
	Ok(written)
}

/// Write one document to stdout without adding framing or a trailing newline.
fn write_verbatim(content: &str) -> MonochangeResult<()> {
	let stdout = std::io::stdout();
	let mut handle = stdout.lock();
	write_verbatim_to(&mut handle, content)
}

/// Write a document verbatim to any writer.
///
/// Split out from [`write_verbatim`] so the write and flush failures are
/// reachable from a test with a failing writer rather than only from a closed
/// stdout.
fn write_verbatim_to(writer: &mut dyn std::io::Write, content: &str) -> MonochangeResult<()> {
	writer.write_all(content.as_bytes()).map_err(|error| {
		MonochangeError::Io(format!("failed to write the skill document: {error}"))
	})?;
	writer.flush().map_err(|error| {
		MonochangeError::Io(format!("failed to flush the skill document: {error}"))
	})
}

fn missing_destination_message() -> String {
	let suggestions = suggest_destinations();
	let mut message = String::from(
		"no skill destination; pass --dir <directory>, for example:\n  monochange skill install --dir ./.claude/skills/monochange\n",
	);
	if suggestions.is_empty() {
		for runtime in [".claude", ".codex", ".agents"] {
			// Writing to a `String` cannot fail.
			let _ = writeln!(
				message,
				"  monochange skill install --dir ~/{runtime}/skills/monochange"
			);
		}
	} else {
		for suggestion in suggestions {
			// Writing to a `String` cannot fail.
			let _ = writeln!(
				message,
				"  monochange skill install --dir {}",
				suggestion.display()
			);
		}
	}
	message.trim_end().to_string()
}

/// Skill directories of the agent runtimes this machine is known to use.
///
/// `monochange skill install` prints these when it is called without `--dir`,
/// so the caller can copy the right path instead of memorizing each runtime's
/// layout.
pub(crate) fn suggest_destinations() -> Vec<PathBuf> {
	let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
		return Vec::new();
	};
	[".claude", ".codex", ".agents"]
		.iter()
		.map(|runtime| home.join(runtime).join("skills").join("monochange"))
		.collect()
}

#[cfg(test)]
#[path = "__tests__/skill_tests.rs"]
mod tests;
