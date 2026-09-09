use std::env;
use std::ffi::OsString;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProgressFormat {
	Auto,
	Unicode,
	Ascii,
	Json,
}

impl ProgressFormat {
	pub(crate) fn parse(value: &str) -> Option<Self> {
		match value {
			"auto" => Some(Self::Auto),
			"unicode" => Some(Self::Unicode),
			"ascii" => Some(Self::Ascii),
			"json" => Some(Self::Json),
			_ => None,
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgressSettings {
	pub(crate) quiet: bool,
	pub(crate) format: ProgressFormat,
	pub(crate) tracing_enabled: bool,
}

impl ProgressSettings {
	pub(crate) fn from_args(args: &[OsString]) -> Self {
		let quiet = args
			.iter()
			.any(|argument| matches!(argument.to_str(), Some("--quiet" | "-q")));
		let tracing_enabled = args
			.iter()
			.filter_map(|argument| argument.to_str())
			.any(|argument| argument == "--log-level" || argument.starts_with("--log-level="));
		let mut format = env::var("MONOCHANGE_PROGRESS_FORMAT")
			.ok()
			.as_deref()
			.and_then(ProgressFormat::parse)
			.unwrap_or(ProgressFormat::Auto);
		let mut arguments = args.iter().filter_map(|argument| argument.to_str());
		while let Some(argument) = arguments.next() {
			if argument == "--progress-format" {
				if let Some(value) = arguments.next().and_then(ProgressFormat::parse) {
					format = value;
				}
				continue;
			}
			if let Some(value) = argument.strip_prefix("--progress-format=")
				&& let Some(value) = ProgressFormat::parse(value)
			{
				format = value;
			}
		}

		Self {
			quiet,
			format,
			tracing_enabled,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalCapabilities {
	pub(crate) stdout_is_terminal: bool,
	pub(crate) stderr_is_terminal: bool,
	pub(crate) ci: bool,
	pub(crate) quiet: bool,
	pub(crate) color: bool,
	pub(crate) animate: bool,
	pub(crate) progress_enabled: bool,
}

impl TerminalCapabilities {
	pub(crate) fn detect(settings: ProgressSettings) -> Self {
		let probe = TerminalProbe {
			stdout_is_terminal: io::stdout().is_terminal(),
			stderr_is_terminal: io::stderr().is_terminal(),
			ci: running_in_ci() && !running_under_test(),
			term_is_dumb: env::var("TERM").is_ok_and(|term| term == "dumb"),
			no_color: env::var_os("NO_COLOR").is_some(),
			no_progress: env::var_os("MONOCHANGE_NO_PROGRESS").is_some(),
		};
		Self::resolve(settings, probe)
	}

	fn resolve(settings: ProgressSettings, probe: TerminalProbe) -> Self {
		let human = settings.format != ProgressFormat::Json;
		let progress_enabled = !settings.quiet && !probe.no_progress;
		let color = progress_enabled
			&& human && probe.stderr_is_terminal
			&& !probe.term_is_dumb
			&& !probe.no_color;
		let animate = progress_enabled
			&& human && probe.stderr_is_terminal
			&& !probe.term_is_dumb
			&& !probe.ci
			&& !settings.tracing_enabled;

		Self {
			stdout_is_terminal: probe.stdout_is_terminal,
			stderr_is_terminal: probe.stderr_is_terminal,
			ci: probe.ci,
			quiet: settings.quiet,
			color,
			animate,
			progress_enabled,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct TerminalProbe {
	stdout_is_terminal: bool,
	stderr_is_terminal: bool,
	ci: bool,
	term_is_dumb: bool,
	no_color: bool,
	no_progress: bool,
}

#[derive(Clone)]
pub(crate) struct SharedStderr {
	writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl SharedStderr {
	pub(crate) fn stdio() -> Self {
		Self::with_writer(io::stderr())
	}

	pub(crate) fn with_writer(writer: impl Write + Send + 'static) -> Self {
		Self {
			writer: Arc::new(Mutex::new(Box::new(writer))),
		}
	}

	pub(crate) fn write(&self, bytes: &[u8]) {
		let Ok(mut writer) = self.writer.lock() else {
			return;
		};
		let _ = writer.write_all(bytes);
		let _ = writer.flush();
	}
}

fn running_in_ci() -> bool {
	[
		"CI",
		"GITHUB_ACTIONS",
		"GITLAB_CI",
		"BUILDKITE",
		"CIRCLECI",
		"TF_BUILD",
	]
	.iter()
	.any(|name| env::var_os(name).is_some())
}

fn running_under_test() -> bool {
	[
		"CARGO_NEXTEST",
		"NEXTEST",
		"INSTA_WORKSPACE_ROOT",
		"INSTA_UPDATE",
	]
	.iter()
	.any(|name| env::var_os(name).is_some())
}

#[cfg(test)]
#[path = "../__tests__/output_terminal_tests.rs"]
mod tests;
