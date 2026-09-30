use std::fmt::Write as _;
use std::path::Path;

use monochange_core::MonochangeError;

/// A failure explained for people: what failed, why, where, and what to do.
///
/// The first rendered line is always `error[<code>]: <summary>`, so CI logs
/// stay greppable by code. A multi-line cause, such as captured command output
/// or an annotated source snippet, renders as a block under the summary,
/// followed by aligned context (`command`, `step`, `path`) and hints.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CliDiagnostic {
	code: &'static str,
	summary: String,
	body: Option<DiagnosticBody>,
	context: Vec<(&'static str, String)>,
	hints: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DiagnosticBody {
	text: String,
	/// Pre-rendered diagnostics (for example `--> monochange.toml:3:1` source
	/// snippets) carry their own alignment, so they render without re-indenting.
	verbatim: bool,
}

impl CliDiagnostic {
	pub(crate) fn from_error(error: &MonochangeError, command: Option<&str>) -> Self {
		let (code, summary, hints) = classify(error);
		let pre_rendered = matches!(error, MonochangeError::Diagnostic(_));
		let (summary, body) = split_summary(&summary, pre_rendered);
		let mut context = Vec::new();
		if let Some(command) = command.filter(|command| !command.is_empty()) {
			context.push(("command", command.to_string()));
		}
		match error {
			MonochangeError::IoSource { path, .. } | MonochangeError::Parse { path, .. } => {
				context.push(("path", path.display().to_string()));
			}
			_ => {}
		}

		Self {
			code,
			summary,
			body,
			context,
			hints,
		}
	}

	/// Show paths inside the workspace relative to its root.
	#[must_use]
	pub(crate) fn with_root(mut self, root: &Path) -> Self {
		let Some(prefix) = root
			.is_absolute()
			.then(|| format!("{}/", root.display().to_string().trim_end_matches('/')))
		else {
			return self;
		};
		self.summary = self.summary.replace(&prefix, "");
		if let Some(body) = &mut self.body {
			body.text = body.text.replace(&prefix, "");
		}
		for (_, value) in &mut self.context {
			*value = value.replace(&prefix, "");
		}
		self
	}

	/// Name the workflow step that failed, after the command context.
	#[must_use]
	pub(crate) fn with_step(mut self, step: String) -> Self {
		let position = self
			.context
			.iter()
			.position(|(label, _)| *label == "command")
			.map_or(0, |index| index + 1);
		self.context.insert(position, ("step", step));
		self
	}

	/// Title for a CI annotation, such as `monochange run release failed`.
	pub(crate) fn annotation_title(&self) -> String {
		self.context
			.iter()
			.find(|(label, _)| *label == "command")
			.map_or_else(
				|| "monochange failed".to_string(),
				|(_, command)| format!("{command} failed"),
			)
	}

	/// A compact plain-text version of the diagnostic for a CI annotation.
	pub(crate) fn annotation(&self) -> String {
		let mut output = format!(
			"error[{}]: {}",
			self.code,
			strip_terminal_controls(&self.summary)
		);
		for (label, value) in &self.context {
			if *label != "command" {
				let _ = write!(output, "\n{label}: {}", strip_terminal_controls(value));
			}
		}
		for hint in &self.hints {
			let _ = write!(output, "\nhelp: {hint}");
		}
		output
	}

	pub(crate) fn render(&self, color: bool) -> String {
		let clean = |text: &str| {
			if color {
				text.to_string()
			} else {
				strip_terminal_controls(text)
			}
		};
		let heading = format!("error[{}]", self.code);
		let mut lines = vec![format!(
			"{}: {}",
			paint(&heading, "31;1", color),
			paint(&clean(&self.summary), "1", color)
		)];

		if let Some(body) = &self.body {
			let text = clean(&body.text);
			// A source location (`--> file:line:col`) belongs directly under the
			// summary, the way compilers print it.
			if !(body.verbatim && text.trim_start().starts_with("-->")) {
				lines.push(String::new());
			}
			let mut previous_was_location = false;
			for line in text.lines() {
				let is_location = body.verbatim && line.trim_start().starts_with("-->");
				if line.trim().is_empty() {
					if !previous_was_location {
						lines.push(String::new());
					}
				} else if body.verbatim {
					lines.push(colorize_snippet_line(line, color));
				} else {
					lines.push(format!("    {line}"));
				}
				previous_was_location = is_location;
			}
		}

		let rows = self
			.context
			.iter()
			.map(|(label, value)| (*label, clean(value)))
			.chain(self.hints.iter().map(|hint| ("help", hint.clone())))
			.collect::<Vec<_>>();
		if !rows.is_empty() && self.body.is_some() {
			lines.push(String::new());
		}
		let width = rows
			.iter()
			.map(|(label, _)| label.len())
			.max()
			.unwrap_or_default();
		for (label, value) in rows {
			let padding = " ".repeat(width - label.len());
			let mut value_lines = value.lines();
			let first = value_lines.next().unwrap_or_default();
			lines.push(format!(
				"  {}{padding} {first}",
				paint(&format!("{label}:"), "36;1", color)
			));
			for line in value_lines {
				lines.push(if line.trim().is_empty() {
					String::new()
				} else {
					format!("  {}  {line}", " ".repeat(width))
				});
			}
		}
		trim_line_ends(&lines.join("\n"))
	}
}

fn classify(error: &MonochangeError) -> (&'static str, String, Vec<String>) {
	match error {
		MonochangeError::Io(message) => {
			(
				"io.failed",
				message.clone(),
				vec![
					"Check that the path exists and is writable, then rerun the command."
						.to_string(),
				],
			)
		}
		MonochangeError::Config(message) if message.contains("--jq requires explicit JSON") => {
			(
				"cli.json_required",
				message.clone(),
				vec!["Add `--format json` or `--format json-min` before using `--jq`.".to_string()],
			)
		}
		MonochangeError::Config(message) if message.contains("check failed") => {
			(
				"check.failed",
				message.clone(),
				vec![
					"Fix the issues reported above, then run `monochange check` again.".to_string(),
				],
			)
		}
		MonochangeError::Config(message)
			if message.contains("did not match any discovered package") =>
		{
			(
				"config.unknown_package",
				message.clone(),
				vec![
					"Run `monochange discover` to list package ids, then use one of them."
						.to_string(),
				],
			)
		}
		MonochangeError::Config(message) if message.starts_with("failed to parse ") => {
			(
				"config.parse_failed",
				message.clone(),
				vec!["Fix the syntax error shown above, then rerun the command.".to_string()],
			)
		}
		MonochangeError::Config(message) => {
			(
				"config.invalid",
				message.clone(),
				vec![
					"Check `monochange.toml` and the command arguments, then rerun the command."
						.to_string(),
				],
			)
		}
		MonochangeError::Discovery(message) if is_command_failure(message) => {
			(
				"step.command_failed",
				message.clone(),
				vec![
					"Fix the failure shown in the command output, then rerun. To reproduce it on \
					 its own, run the command directly from the workspace root."
						.to_string(),
				],
			)
		}
		MonochangeError::Discovery(message) => {
			(
				"workspace.discovery_failed",
				message.clone(),
				vec![
					"Check the configured package paths and workspace manifests, then rerun the \
					 command."
						.to_string(),
				],
			)
		}
		MonochangeError::Diagnostic(message) => {
			(diagnostic_code(message), message.clone(), Vec::new())
		}
		MonochangeError::Reported { diagnostic, .. } => {
			// The command result printed above already lists the failing items,
			// so a generic hint would only repeat that.
			let code = if diagnostic.contains("check failed") {
				"check.failed"
			} else {
				"command.failed"
			};
			(code, diagnostic.clone(), Vec::new())
		}
		MonochangeError::IoSource { path: _, source } => {
			(
				"file.io_failed",
				format!("{source}"),
				vec![
					"Check that the file exists and that monochange can read or write it."
						.to_string(),
				],
			)
		}
		MonochangeError::Parse { path: _, source } => {
			(
				"file.parse_failed",
				format!("{source}"),
				vec!["Fix the invalid file contents, then rerun the command.".to_string()],
			)
		}
		MonochangeError::Interactive { message } => {
			(
				"interactive.failed",
				message.clone(),
				vec![
					"Rerun in an interactive terminal or provide the required values as flags."
						.to_string(),
				],
			)
		}
		MonochangeError::Cancelled => {
			(
				"operation.cancelled",
				"The operation was cancelled.".to_string(),
				vec!["Rerun the command when you are ready to continue.".to_string()],
			)
		}
		_ => {
			(
				"command.failed",
				error.render(),
				vec![
					"Rerun with `--log-level debug` to trace the requests monochange made."
						.to_string(),
				],
			)
		}
	}
}

/// Pre-rendered diagnostics come from config validation, changeset validation,
/// and command-line parsing; name each so CI searches find the right family.
fn diagnostic_code(message: &str) -> &'static str {
	if message.contains("\nUsage: ") || message.contains("For more information, try '--help'") {
		"cli.usage"
	} else if message.starts_with("changeset target validation failed") {
		"changeset.invalid"
	} else if message.starts_with("error: ") {
		"config.invalid"
	} else {
		"cli.diagnostic"
	}
}

fn is_command_failure(message: &str) -> bool {
	message.starts_with("command `") && message.contains("` failed: ")
}

fn split_summary(message: &str, pre_rendered: bool) -> (String, Option<DiagnosticBody>) {
	let message = if pre_rendered {
		message.strip_prefix("error: ").unwrap_or(message)
	} else {
		message
	};
	let (summary, body) = message
		.split_once('\n')
		.map_or((message, None), |(summary, body)| (summary, Some(body)));
	let body = body
		.map(|body| body.trim_matches('\n'))
		.filter(|body| !body.trim().is_empty())
		.map(|body| {
			DiagnosticBody {
				text: body.to_string(),
				verbatim: pre_rendered,
			}
		});
	(summary.trim().to_string(), body)
}

/// Colour the gutter, location arrow, carets, and notes of a source snippet.
fn colorize_snippet_line(line: &str, color: bool) -> String {
	if !color {
		return line.to_string();
	}
	let trimmed = line.trim_start();
	let indent = &line[..line.len() - trimmed.len()];
	if let Some(rest) = trimmed.strip_prefix("-->") {
		return format!("{indent}{}{rest}", paint("-->", "36;1", true));
	}
	for prefix in ["= help:", "= note:"] {
		if let Some(rest) = trimmed.strip_prefix(prefix) {
			return format!("{indent}{}{rest}", paint(prefix, "36;1", true));
		}
	}
	let Some((gutter, source)) = line.split_once('|') else {
		return line.to_string();
	};
	if !gutter
		.trim()
		.chars()
		.all(|character| character.is_ascii_digit())
	{
		return line.to_string();
	}
	let source_trimmed = source.trim_start();
	let source = if source_trimmed.starts_with('^') {
		format!(
			"{}{}",
			&source[..source.len() - source_trimmed.len()],
			paint(source_trimmed, "31;1", true)
		)
	} else {
		source.to_string()
	};
	format!("{}{source}", paint(&format!("{gutter}|"), "36;1", true))
}

fn trim_line_ends(text: &str) -> String {
	text.lines()
		.map(str::trim_end)
		.collect::<Vec<_>>()
		.join("\n")
		.trim_end()
		.to_string()
}

fn paint(text: &str, code: &str, color: bool) -> String {
	if color {
		format!("\u{1b}[{code}m{text}\u{1b}[0m")
	} else {
		text.to_string()
	}
}

fn strip_terminal_controls(text: &str) -> String {
	crate::output::progress::strip_terminal_controls(text)
}

#[cfg(test)]
#[path = "../__tests__/output_diagnostic_tests.rs"]
mod tests;
