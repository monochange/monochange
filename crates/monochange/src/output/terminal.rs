use std::env;
use std::ffi::OsString;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

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
	/// `--verbose`: show every phase timing and progress notes.
	pub(crate) verbose: bool,
}

impl ProgressSettings {
	pub(crate) fn from_args(args: &[OsString]) -> Self {
		let quiet = args
			.iter()
			.any(|argument| matches!(argument.to_str(), Some("--quiet" | "-q")));
		let verbose = verbose_requested(args);
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
			verbose,
		}
	}
}

/// Whether the command line asks for `--verbose` output.
pub(crate) fn verbose_requested(args: &[OsString]) -> bool {
	args.iter()
		.any(|argument| matches!(argument.to_str(), Some("--verbose" | "-v")))
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
	/// GitHub Actions is running this process and progress is enabled, so
	/// human progress may use workflow commands to fold command output and
	/// annotate failures.
	pub(crate) github_actions: bool,
	pub(crate) verbose: bool,
}

impl TerminalCapabilities {
	pub(crate) fn detect(settings: ProgressSettings) -> Self {
		let probe = TerminalProbe {
			stdout_is_terminal: io::stdout().is_terminal(),
			stderr_is_terminal: io::stderr().is_terminal(),
			ci: running_in_ci() && !running_under_test(),
			github_actions: running_in_github_actions() && !running_under_test(),
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
			// `MONOCHANGE_NO_PROGRESS` and `--quiet` also turn off workflow
			// commands, so logs stay plain when progress output is disabled.
			github_actions: human && progress_enabled && probe.github_actions,
			verbose: settings.verbose,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct TerminalProbe {
	stdout_is_terminal: bool,
	stderr_is_terminal: bool,
	ci: bool,
	github_actions: bool,
	term_is_dumb: bool,
	no_color: bool,
	no_progress: bool,
}

/// The one stderr channel shared by progress, diagnostics, and warnings.
///
/// Every writer goes through this handle so a line written while the spinner
/// is animating first erases the spinner's partial line instead of splicing
/// text into it, and so JSON events from different writers share one sequence.
#[derive(Clone)]
pub(crate) struct SharedStderr {
	writer: Arc<Mutex<Box<dyn Write + Send>>>,
	spinner_active: Arc<AtomicBool>,
	line_cleared: Arc<AtomicBool>,
	sequence: Arc<AtomicU64>,
	group_open: Arc<AtomicBool>,
}

impl SharedStderr {
	pub(crate) fn stdio() -> Self {
		Self::with_writer(io::stderr())
	}

	pub(crate) fn with_writer(writer: impl Write + Send + 'static) -> Self {
		Self {
			writer: Arc::new(Mutex::new(Box::new(writer))),
			spinner_active: Arc::new(AtomicBool::new(false)),
			line_cleared: Arc::new(AtomicBool::new(false)),
			sequence: Arc::new(AtomicU64::new(0)),
			group_open: Arc::new(AtomicBool::new(false)),
		}
	}

	pub(crate) fn write(&self, bytes: &[u8]) {
		let Ok(mut writer) = self.writer.lock() else {
			return;
		};
		let _ = writer.write_all(bytes);
		let _ = writer.flush();
	}

	/// Write one complete line, erasing an animated spinner line first.
	pub(crate) fn write_line(&self, text: &str) {
		if self.spinner_active.load(Ordering::Relaxed) {
			self.write(format!("\r\u{1b}[2K\u{1b}[0m{text}\n").as_bytes());
			self.line_cleared.store(true, Ordering::Relaxed);
			return;
		}
		self.write(format!("{text}\n").as_bytes());
	}

	pub(crate) fn set_spinner_active(&self, active: bool) {
		self.spinner_active.store(active, Ordering::Relaxed);
	}

	/// Report whether another writer erased the spinner line since the last
	/// call, so the spinner knows to repaint its whole message.
	pub(crate) fn take_line_cleared(&self) -> bool {
		self.line_cleared.swap(false, Ordering::Relaxed)
	}

	pub(crate) fn next_sequence(&self) -> u64 {
		self.sequence.fetch_add(1, Ordering::Relaxed)
	}

	/// Start a folded GitHub Actions log group unless one is already open.
	/// `title` must already be escaped for a workflow command.
	pub(crate) fn open_group(&self, title: &str) {
		if !self.group_open.swap(true, Ordering::Relaxed) {
			self.write_line(&format!("::group::{title}"));
		}
	}

	/// End the open GitHub Actions log group, so the next line stays visible.
	pub(crate) fn close_group(&self) {
		if self.group_open.swap(false, Ordering::Relaxed) {
			self.write_line("::endgroup::");
		}
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

fn running_in_github_actions() -> bool {
	env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true")
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
