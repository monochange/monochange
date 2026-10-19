//! Contributor tasks for the experimental shared compute flow.
//!
//! A project with more accepted work than compute can share a task, and a
//! contributor runs it with their own agent. The task is the only thing that
//! agent reads: a title, acceptance criteria, the paths it may change, and the
//! command that proves it, all pinned to one public commit. The issue and its
//! discussion stay out of it, because they quote end users.
//!
//! [`screen_task`] checks a task with rules a program can enforce before
//! anyone can run it. Rules are a tripwire, not the guarantee: the
//! contributor's sandbox is what makes a hostile task harmless, so the rules
//! favour refusing a borderline task over listing it.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::pipeline::Stage;
use crate::triage::ScreeningVerdict;
use crate::triage::screen_untrusted;

/// The longest title a task may have.
pub const MAX_TITLE: usize = 120;
/// The longest single acceptance criterion.
pub const MAX_CRITERION: usize = 300;
/// The most acceptance criteria a task may list.
pub const MAX_CRITERIA: usize = 10;
/// The most paths a task may change.
pub const MAX_PATHS: usize = 20;
/// The longest path pattern.
pub const MAX_PATH: usize = 200;
/// The longest check command.
pub const MAX_CHECK: usize = 200;
/// The most tasks a project may list at once.
pub const MAX_LISTED_TASKS: usize = 20;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TaskError {
	#[error("shared compute is off for this project")]
	NotEnabled,
	#[error("only projects whose repositories are all public can share tasks")]
	PrivateProject,
	#[error("only accepted items can be shared, but this one is {0:?}")]
	NotAccepted(Stage),
	#[error("link a GitHub issue that names its repository before sharing a task")]
	IssueRequired,
	#[error("this project already lists {MAX_LISTED_TASKS} tasks; withdraw one first")]
	TooManyListed,
	#[error("task {0} does not exist")]
	NotFound(String),
	#[error("task {0} was already withdrawn")]
	AlreadyWithdrawn(String),
}

/// What a maintainer writes when sharing a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDraft {
	pub title: String,
	pub acceptance: Vec<String>,
	/// Repository-relative paths or globs the work may change, such as
	/// `crates/invoices/**`.
	pub paths: Vec<String>,
	/// The one command that proves the work, such as `cargo test -p invoices`.
	pub check: String,
}

impl TaskDraft {
	/// The draft as screening and contributors see it: every field trimmed,
	/// and blank criteria and paths dropped.
	#[must_use]
	pub fn normalized(&self) -> Self {
		let lines = |values: &[String]| {
			values
				.iter()
				.map(|value| value.trim().to_owned())
				.filter(|value| !value.is_empty())
				.collect()
		};

		Self {
			title: self.title.trim().to_owned(),
			acceptance: lines(&self.acceptance),
			paths: lines(&self.paths),
			check: self.check.trim().to_owned(),
		}
	}
}

/// Where a task runs: the repository, the commit it is pinned to, and the
/// issue it implements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTarget {
	/// `owner/name`.
	pub repository: String,
	/// The full 40-character commit SHA.
	pub commit: String,
	pub issue_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskStatus {
	/// Passed screening; contributors can run it until it expires.
	Listed,
	/// Failed screening and was never listed. Only maintainers see it.
	Rejected { reasons: Vec<TaskRejection> },
	/// A maintainer took it down.
	Withdrawn,
}

/// A task on an accepted item, listed or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributorTask {
	/// `task-{n}`, unique within the project.
	pub id: String,
	/// The feedback item the task delivers.
	pub item: String,
	pub target: TaskTarget,
	pub draft: TaskDraft,
	/// The maintainer who shared it.
	pub shared_by: String,
	/// Unix seconds.
	pub created_at: i64,
	/// Unix seconds; the task stops listing at this time.
	pub expires_at: i64,
	pub status: TaskStatus,
}

/// A listed task exactly as a contributor's agent receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTask {
	pub id: String,
	pub item: String,
	pub repository: String,
	pub commit: String,
	pub issue_number: u64,
	pub title: String,
	pub acceptance: Vec<String>,
	pub paths: Vec<String>,
	pub check: String,
	pub expires_at: i64,
}

impl PublicTask {
	pub(crate) fn of(task: &ContributorTask) -> Self {
		Self {
			id: task.id.clone(),
			item: task.item.clone(),
			repository: task.target.repository.clone(),
			commit: task.target.commit.clone(),
			issue_number: task.target.issue_number,
			title: task.draft.title.clone(),
			acceptance: task.draft.acceptance.clone(),
			paths: task.draft.paths.clone(),
			check: task.draft.check.clone(),
			expires_at: task.expires_at,
		}
	}
}

/// The part of a task a rule refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskField {
	Title,
	Acceptance,
	Paths,
}

impl fmt::Display for TaskField {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Title => "the title",
			Self::Acceptance => "the acceptance criteria",
			Self::Paths => "the paths",
		})
	}
}

/// Why screening refused a task. Each reason names what to change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum TaskRejection {
	Missing {
		field: TaskField,
	},
	TooLong {
		field: TaskField,
		limit: usize,
	},
	TooMany {
		field: TaskField,
		limit: usize,
	},
	InvalidCommit,
	/// The text asks for something on the contributor's machine.
	AsksForSecrets {
		field: TaskField,
		phrase: String,
	},
	/// The text links somewhere other than the task's repository.
	LinksOutside {
		field: TaskField,
		url: String,
	},
	/// Invisible characters, control characters, or an encoded blob.
	HiddenContent {
		field: TaskField,
	},
	/// The text reads like instructions to the agent's safety rules.
	Injection {
		field: TaskField,
		marker: String,
	},
	/// A path leaves the repository or isn't a plain path or glob.
	UnsafePath {
		path: String,
	},
	/// A path covers CI configuration or git internals.
	ProtectedPath {
		path: String,
	},
	/// The check isn't one plain command from a known build tool.
	UnsupportedCheck {
		reason: String,
	},
}

impl fmt::Display for TaskRejection {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Missing { field } => write!(formatter, "{field} can't be empty"),
			Self::TooLong {
				field: TaskField::Acceptance,
				limit,
			} => {
				write!(
					formatter,
					"each acceptance criterion can be at most {limit} characters"
				)
			}
			Self::TooLong { field, limit } => {
				write!(formatter, "{field} can be at most {limit} characters")
			}
			Self::TooMany { field, limit } => {
				write!(
					formatter,
					"a task can list at most {limit} {field}",
					field = match field {
						TaskField::Paths => "paths",
						_ => "acceptance criteria",
					}
				)
			}
			Self::InvalidCommit => {
				formatter.write_str("the commit must be a full 40-character SHA")
			}
			Self::AsksForSecrets { field, phrase } => {
				write!(
					formatter,
					"“{phrase}” in {field} points at the contributor's machine, which tasks can't ask about"
				)
			}
			Self::LinksOutside { field, url } => {
				write!(
					formatter,
					"{url} in {field} points outside the task's repository"
				)
			}
			Self::HiddenContent { field } => {
				write!(
					formatter,
					"{field} can't contain line breaks, invisible characters, or encoded blobs"
				)
			}
			Self::Injection { field, marker } => {
				write!(
					formatter,
					"“{marker}” in {field} reads as instructions to the agent"
				)
			}
			Self::UnsafePath { path } => {
				write!(formatter, "{path} isn't a plain path inside the repository")
			}
			Self::ProtectedPath { path } => {
				write!(
					formatter,
					"{path} is CI configuration or repository internals, which tasks can't change"
				)
			}
			Self::UnsupportedCheck { reason } => write!(formatter, "the check command {reason}"),
		}
	}
}

/// Phrases that point at the contributor's machine rather than the code.
/// Matched case-insensitively anywhere in the text.
const SECRET_PHRASES: [&str; 24] = [
	"environment variable",
	"env var",
	"printenv",
	"process.env",
	"os.environ",
	"std::env",
	"credential",
	"api key",
	"api_key",
	"apikey",
	"access token",
	"auth token",
	"secret key",
	"private key",
	"ssh key",
	"id_rsa",
	"id_ed25519",
	".ssh",
	".aws",
	".npmrc",
	".netrc",
	"keychain",
	"home directory",
	"$home",
];

/// Paths that start with any of these are never a task's to change.
const PROTECTED_PREFIXES: [&str; 3] = [".github", ".git", ".monochange"];

/// Programs a check command may start with.
const CHECK_PROGRAMS: [&str; 25] = [
	"bun", "bundle", "cargo", "dart", "deno", "dotnet", "flutter", "go", "gradle", "just", "make",
	"mix", "mvn", "npm", "pnpm", "poetry", "pytest", "python", "python3", "rake", "swift", "tox",
	"uv", "yarn", "zig",
];

/// Arguments that turn a build tool into an interpreter for inline code.
const INLINE_CODE_FLAGS: [&str; 5] = ["-c", "-e", "--eval", "--command", "--exec"];

/// The shortest run of base64 or hex characters treated as an encoded blob.
const BLOB_LENGTH: usize = 64;

/// Checks a task before it is listed. An empty result means it passed.
pub fn screen_task(target: &TaskTarget, draft: &TaskDraft) -> Vec<TaskRejection> {
	let mut rejections = Vec::new();

	if !is_commit_sha(&target.commit) {
		rejections.push(TaskRejection::InvalidCommit);
	}

	screen_text(
		TaskField::Title,
		std::slice::from_ref(&draft.title),
		MAX_TITLE,
		1,
		&target.repository,
		&mut rejections,
	);
	screen_text(
		TaskField::Acceptance,
		&draft.acceptance,
		MAX_CRITERION,
		MAX_CRITERIA,
		&target.repository,
		&mut rejections,
	);
	screen_paths(&draft.paths, &mut rejections);

	if let Err(reason) = check_command(&draft.check) {
		rejections.push(TaskRejection::UnsupportedCheck { reason });
	}

	rejections
}

fn is_commit_sha(commit: &str) -> bool {
	commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn screen_text(
	field: TaskField,
	values: &[String],
	max_length: usize,
	max_count: usize,
	repository: &str,
	rejections: &mut Vec<TaskRejection>,
) {
	if values.iter().all(|value| value.trim().is_empty()) {
		rejections.push(TaskRejection::Missing { field });
		return;
	}

	if values.len() > max_count {
		rejections.push(TaskRejection::TooMany {
			field,
			limit: max_count,
		});
	}

	if values
		.iter()
		.any(|value| value.chars().count() > max_length)
	{
		rejections.push(TaskRejection::TooLong {
			field,
			limit: max_length,
		});
	}

	if values.iter().any(|value| has_hidden_content(value)) {
		rejections.push(TaskRejection::HiddenContent { field });
	}

	let text = values.join("\n");
	let lower = text.to_lowercase();

	if let Some(phrase) = SECRET_PHRASES
		.into_iter()
		.find(|phrase| lower.contains(phrase))
	{
		rejections.push(TaskRejection::AsksForSecrets {
			field,
			phrase: phrase.to_owned(),
		});
	}

	if let Some(url) = outside_link(&text, repository) {
		rejections.push(TaskRejection::LinksOutside { field, url });
	}

	if let ScreeningVerdict::InjectionSuspected { markers } = screen_untrusted(&text)
		&& let Some(marker) = markers.into_iter().next()
	{
		rejections.push(TaskRejection::Injection { field, marker });
	}
}

/// Characters that render as nothing or reorder text, so what a contributor
/// reads could differ from what their agent receives.
fn is_invisible(character: char) -> bool {
	matches!(
		character,
		'\u{00AD}'
			| '\u{034F}'
			| '\u{061C}'
			| '\u{115F}'
			| '\u{1160}'
			| '\u{17B4}'
			| '\u{17B5}'
			| '\u{180B}'..='\u{180F}'
			| '\u{200B}'..='\u{200F}'
			| '\u{202A}'..='\u{202E}'
			| '\u{2060}'..='\u{206F}'
			| '\u{3164}'
			| '\u{FE00}'..='\u{FE0F}'
			| '\u{FEFF}'
			| '\u{FFA0}'
			| '\u{FFF9}'..='\u{FFFB}'
			| '\u{E0000}'..='\u{E0FFF}'
	)
}

fn has_hidden_content(value: &str) -> bool {
	value
		.chars()
		.any(|character| character.is_control() || is_invisible(character))
		|| value.split_whitespace().any(|word| {
			word.len() >= BLOB_LENGTH
				&& word.bytes().all(|byte| {
					byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'_' | b'-')
				})
		})
}

/// The first link in `text` that doesn't point into `repository` on GitHub.
fn outside_link(text: &str, repository: &str) -> Option<String> {
	let own = format!("https://github.com/{}", repository.to_lowercase());

	text.split_whitespace()
		.map(|word| word.trim_matches(|character: char| "()[]<>\"'`,.;:!?".contains(character)))
		.filter(|word| {
			let lower = word.to_lowercase();
			lower.contains("://") || lower.starts_with("www.")
		})
		.find(|word| {
			let lower = word.to_lowercase();
			let rest = lower.strip_prefix(&own);
			!rest.is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '#', '?']))
		})
		.map(str::to_owned)
}

fn screen_paths(paths: &[String], rejections: &mut Vec<TaskRejection>) {
	if paths.iter().all(|path| path.trim().is_empty()) {
		rejections.push(TaskRejection::Missing {
			field: TaskField::Paths,
		});
		return;
	}

	if paths.len() > MAX_PATHS {
		rejections.push(TaskRejection::TooMany {
			field: TaskField::Paths,
			limit: MAX_PATHS,
		});
	}

	for path in paths {
		if !is_plain_path(path) {
			rejections.push(TaskRejection::UnsafePath { path: path.clone() });
		} else if is_protected(path) {
			rejections.push(TaskRejection::ProtectedPath { path: path.clone() });
		}
	}
}

/// A repository-relative path or glob with at least one literal segment
/// before any wildcard, so `**` alone can't grant the whole repository.
fn is_plain_path(path: &str) -> bool {
	let segments: Vec<&str> = path.split('/').collect();

	!path.is_empty()
		&& path.len() <= MAX_PATH
		&& path.bytes().all(|byte| {
			byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/' | b'*' | b'@')
		}) && segments
		.iter()
		.all(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
		&& segments.first().is_some_and(|first| !first.contains('*'))
}

fn is_protected(path: &str) -> bool {
	let first = path.split('/').next().unwrap_or_default();
	PROTECTED_PREFIXES
		.into_iter()
		.any(|prefix| first.eq_ignore_ascii_case(prefix))
}

/// One build tool invocation with plain arguments: no shell syntax, no
/// inline code, and nothing outside the repository.
fn check_command(check: &str) -> Result<(), String> {
	let words: Vec<&str> = check.split(' ').filter(|word| !word.is_empty()).collect();
	let Some(program) = words.first() else {
		return Err("can't be empty".to_owned());
	};

	if check.chars().count() > MAX_CHECK {
		return Err(format!("can be at most {MAX_CHECK} characters"));
	}

	if let Some(word) = words.iter().find(|word| {
		!word.bytes().all(|byte| {
			byte.is_ascii_alphanumeric()
				|| matches!(
					byte,
					b'.' | b'_' | b'-' | b'/' | b':' | b'=' | b'@' | b'+' | b','
				)
		})
	}) {
		return Err(format!(
			"can't use shell syntax, quotes, or variables ({word})"
		));
	}

	if !CHECK_PROGRAMS.contains(program) {
		return Err(format!(
			"must start with a build tool such as cargo, npm, or pytest, not {program}"
		));
	}

	if let Some(word) = words.iter().find(|word| {
		word.starts_with('/')
			|| word.contains("=/")
			|| word.split(['/', '=']).any(|segment| segment == "..")
	}) {
		return Err(format!("can't reach outside the repository ({word})"));
	}

	if let Some(flag) = words.iter().find(|word| INLINE_CODE_FLAGS.contains(word)) {
		return Err(format!("can't run inline code ({flag})"));
	}

	Ok(())
}

#[cfg(test)]
#[path = "__tests__/tasks_tests.rs"]
mod tests;
