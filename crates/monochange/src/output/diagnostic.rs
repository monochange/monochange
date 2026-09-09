use std::fmt::Write as _;

use monochange_core::MonochangeError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CliDiagnostic {
	code: &'static str,
	summary: String,
	context: Vec<(&'static str, String)>,
	hints: Vec<String>,
}

impl CliDiagnostic {
	pub(crate) fn from_error(error: &MonochangeError, command: Option<&str>) -> Self {
		let (code, summary, hints) = match error {
			MonochangeError::Io(message) => (
				"io.failed",
				message.clone(),
				vec!["Check that the path exists and is writable, then rerun the command.".to_string()],
			),
			MonochangeError::Config(message) if message.contains("--jq requires explicit JSON") => (
				"cli.json_required",
				message.clone(),
				vec!["Add `--format json` or `--format json-min` before using `--jq`.".to_string()],
			),
			MonochangeError::Config(message) if message.contains("check failed") => (
				"check.failed",
				message.clone(),
				vec!["Review the reported validation or lint issues, fix them, and run `monochange check` again.".to_string()],
			),
			MonochangeError::Config(message) => (
				"config.invalid",
				message.clone(),
				vec!["Check `monochange.toml` and the command arguments, then rerun the command.".to_string()],
			),
			MonochangeError::Discovery(message) => (
				"workspace.discovery_failed",
				message.clone(),
				vec!["Check the configured package paths and workspace manifests, then rerun the command.".to_string()],
			),
			MonochangeError::Diagnostic(message) => {
				("cli.diagnostic", message.clone(), Vec::new())
			}
			MonochangeError::Reported { diagnostic, .. } => {
				let code = if diagnostic.contains("check failed") {
					"check.failed"
				} else {
					"command.failed"
				};
				(
					code,
					diagnostic.clone(),
					vec!["Review the result above for the failing items and suggested fixes.".to_string()],
				)
			}
			MonochangeError::IoSource { path: _, source } => (
				"file.io_failed",
				format!("{source}"),
				vec!["Check that the file exists and that monochange can read or write it.".to_string()],
			),
			MonochangeError::Parse { path: _, source } => (
				"file.parse_failed",
				format!("{source}"),
				vec!["Fix the invalid file contents, then rerun the command.".to_string()],
			),
			MonochangeError::Interactive { message } => (
				"interactive.failed",
				message.clone(),
				vec!["Rerun in an interactive terminal or provide the required values as flags.".to_string()],
			),
			MonochangeError::Cancelled => (
				"operation.cancelled",
				"The operation was cancelled.".to_string(),
				vec!["Rerun the command when you are ready to continue.".to_string()],
			),
			_ => (
				"command.failed",
				error.render(),
				vec!["Rerun with `--log-level debug` if you need maintainer-level diagnostics.".to_string()],
			),
		};
		let (summary, cause) = summary
			.split_once('\n')
			.map_or((summary.as_str(), None), |(summary, cause)| {
				(summary, Some(cause))
			});
		let mut context = Vec::new();
		if let Some(cause) = cause
			.map(|cause| cause.trim_matches('\n'))
			.filter(|cause| !cause.is_empty())
		{
			context.push(("cause", cause.to_string()));
		}
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
			summary: summary.to_string(),
			context,
			hints,
		}
	}

	pub(crate) fn render(&self, color: bool) -> String {
		let mut output = String::new();
		let heading = format!("error[{}]", self.code);
		let summary = if color {
			self.summary.clone()
		} else {
			strip_terminal_controls(&self.summary)
		};
		let _ = writeln!(output, "{}: {}", paint(&heading, "31;1", color), summary);
		for (label, value) in &self.context {
			let value = if color {
				value.clone()
			} else {
				strip_terminal_controls(value)
			};
			let mut lines = value.lines();
			if let Some(first) = lines.next() {
				let _ = writeln!(output, "  {}: {first}", paint(label, "36;1", color));
			}
			for line in lines {
				if line.trim().is_empty() {
					output.push('\n');
				} else {
					let _ = writeln!(output, "    {line}");
				}
			}
		}
		for hint in &self.hints {
			let _ = writeln!(output, "  {}: {hint}", paint("help", "36;1", color));
		}
		output.trim_end().to_string()
	}
}

fn paint(text: &str, code: &str, color: bool) -> String {
	if color {
		format!("\u{1b}[{code}m{text}\u{1b}[0m")
	} else {
		text.to_string()
	}
}

fn strip_terminal_controls(text: &str) -> String {
	let mut output = String::with_capacity(text.len());
	let mut characters = text.chars().peekable();
	while let Some(character) = characters.next() {
		if character != '\u{1b}' {
			if character != '\r' {
				output.push(character);
			}
			continue;
		}
		if characters.next() == Some('[') {
			for control in characters.by_ref() {
				if ('@'..='~').contains(&control) {
					break;
				}
			}
		}
	}
	output
}

#[cfg(test)]
#[path = "../__tests__/output_diagnostic_tests.rs"]
mod tests;
