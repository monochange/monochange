use std::cmp::Reverse;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::lint::LintProgressReporter;
use monochange_publish::EcosystemProgressPresentation;
use monochange_publish::PublishProgressEvent;
use monochange_publish::PublishProgressPackage;
use monochange_publish::PublishProgressReporter;
use serde::Serialize;

use crate::StepPhaseTiming;
use crate::output::diagnostic::CliDiagnostic;
use crate::output::terminal::ProgressFormat;
use crate::output::terminal::ProgressSettings;
use crate::output::terminal::SharedStderr;
use crate::output::terminal::TerminalCapabilities;

const UNICODE_SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASCII_SPINNER_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
const SPINNER_TICK: Duration = Duration::from_millis(90);
const SPINNER_DELAY: Duration = Duration::from_millis(120);
const PHASE_TIMING_DETAIL_LIMIT: usize = 5;
const PHASE_TIMING_MINIMUM: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
pub(crate) enum CommandStream {
	Stdout,
	Stderr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProgressRenderMode {
	Human,
	Json,
}

#[derive(Clone, Copy)]
struct ProgressSymbols {
	command_success: &'static str,
	step_start: &'static str,
	step_skip: &'static str,
	step_success: &'static str,
	step_failure: &'static str,
	error_branch: &'static str,
	bullet: &'static str,
	log_pipe: &'static str,
	spinner_frames: &'static [&'static str],
}

const UNICODE_SYMBOLS: ProgressSymbols = ProgressSymbols {
	command_success: "✓",
	step_start: "▶",
	step_skip: "○",
	step_success: "✔",
	step_failure: "✖",
	error_branch: "└─",
	bullet: "·",
	log_pipe: "│",
	spinner_frames: &UNICODE_SPINNER_FRAMES,
};

const ASCII_SYMBOLS: ProgressSymbols = ProgressSymbols {
	command_success: "+",
	step_start: ">",
	step_skip: "-",
	step_success: "+",
	step_failure: "x",
	error_branch: "`-",
	bullet: "-",
	log_pipe: "|",
	spinner_frames: &ASCII_SPINNER_FRAMES,
};

#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ProgressReporter {
	enabled: bool,
	color: bool,
	animate: bool,
	capabilities: TerminalCapabilities,
	stderr: SharedStderr,
	command_name: String,
	dry_run: bool,
	total_steps: usize,
	active_spinner: Mutex<Option<SpinnerState>>,
	command_started: AtomicBool,
	render_mode: ProgressRenderMode,
	symbols: ProgressSymbols,
	event_sequence: AtomicU64,
	line_cleared: Arc<AtomicBool>,
}

struct SpinnerState {
	stop: Arc<AtomicBool>,
	rendered: Arc<AtomicBool>,
	handle: JoinHandle<()>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
struct ProgressPhaseTiming {
	label: String,
	duration_ms: u64,
}

impl ProgressReporter {
	pub(crate) fn new(
		cli_command: &CliCommandDefinition,
		dry_run: bool,
		quiet: bool,
		format: ProgressFormat,
	) -> Self {
		let settings = ProgressSettings {
			quiet,
			format,
			tracing_enabled: false,
		};
		let capabilities = TerminalCapabilities::detect(settings);
		Self::with_output(
			cli_command,
			dry_run,
			format,
			capabilities,
			SharedStderr::stdio(),
		)
	}

	pub(crate) fn for_invocation(arguments: &[std::ffi::OsString]) -> Self {
		let settings = ProgressSettings::from_args(arguments);
		let capabilities = TerminalCapabilities::detect(settings);
		Self::with_context(
			"monochange".to_string(),
			false,
			0,
			settings.format,
			capabilities,
			SharedStderr::stdio(),
		)
	}

	pub(crate) fn named(
		command_name: &str,
		dry_run: bool,
		quiet: bool,
		format: ProgressFormat,
	) -> Self {
		let settings = ProgressSettings {
			quiet,
			format,
			tracing_enabled: false,
		};
		let capabilities = TerminalCapabilities::detect(settings);
		Self::with_context(
			command_name.to_string(),
			dry_run,
			0,
			format,
			capabilities,
			SharedStderr::stdio(),
		)
	}

	fn with_output(
		cli_command: &CliCommandDefinition,
		dry_run: bool,
		format: ProgressFormat,
		capabilities: TerminalCapabilities,
		stderr: SharedStderr,
	) -> Self {
		Self::with_context(
			cli_command.name.clone(),
			dry_run,
			cli_command.steps.len(),
			format,
			capabilities,
			stderr,
		)
	}

	fn with_context(
		command_name: String,
		dry_run: bool,
		total_steps: usize,
		format: ProgressFormat,
		capabilities: TerminalCapabilities,
		stderr: SharedStderr,
	) -> Self {
		let (render_mode, symbols) = match format {
			ProgressFormat::Auto | ProgressFormat::Unicode => {
				(ProgressRenderMode::Human, UNICODE_SYMBOLS)
			}
			ProgressFormat::Ascii => (ProgressRenderMode::Human, ASCII_SYMBOLS),
			ProgressFormat::Json => (ProgressRenderMode::Json, ASCII_SYMBOLS),
		};
		Self {
			enabled: capabilities.progress_enabled,
			color: capabilities.color,
			animate: capabilities.animate,
			capabilities,
			stderr,
			command_name,
			dry_run,
			total_steps,
			active_spinner: Mutex::new(None),
			command_started: AtomicBool::new(false),
			render_mode,
			symbols,
			event_sequence: AtomicU64::new(0),
			line_cleared: Arc::new(AtomicBool::new(false)),
		}
	}

	pub(crate) fn configure_command(&mut self, cli_command: &CliCommandDefinition, dry_run: bool) {
		self.command_name.clone_from(&cli_command.name);
		self.dry_run = dry_run;
		self.total_steps = cli_command.steps.len();
	}

	pub(crate) fn configure_named_command(&mut self, command_name: &str) {
		self.command_name.clear();
		self.command_name.push_str(command_name);
		self.dry_run = false;
		self.total_steps = 0;
	}

	pub(crate) fn is_enabled(&self) -> bool {
		self.enabled
	}

	pub(crate) fn write_diagnostic(&self, diagnostic: &CliDiagnostic) {
		self.stop_spinner();
		self.stderr
			.write(format!("{}\n", diagnostic.render(self.color)).as_bytes());
	}

	pub(crate) fn phase_started(&self, phase: &str) {
		if !self.enabled {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "phase_started",
				"phase": phase,
			}));
			return;
		}
		if self.animate {
			self.start_spinner(phase.to_string());
		} else {
			self.print_line(&format!(
				"{} {}",
				self.paint(self.symbols.step_start, Style::Accent),
				self.paint(phase, Style::Header),
			));
		}
	}

	pub(crate) fn phase_finished(&self, phase: &str, duration: Duration) {
		if !self.enabled {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "phase_finished",
				"phase": phase,
				"duration_ms": duration_millis(duration),
			}));
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_success, Style::Success),
			self.paint(phase, Style::Header),
			self.paint(&format_duration(duration), Style::Muted),
		));
	}

	pub(crate) fn phase_failed(&self, phase: &str, duration: Duration, error: &str) {
		if !self.enabled {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "phase_failed",
				"phase": phase,
				"duration_ms": duration_millis(duration),
				"error": error,
			}));
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_failure, Style::Error),
			self.paint(phase, Style::Header),
			self.paint(&format_duration(duration), Style::Muted),
		));
	}

	pub(crate) fn warning(&self, message: &str) {
		if self.capabilities.quiet {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"warning",
				serde_json::json!({ "message": message })
					.as_object()
					.cloned()
					.unwrap_or_default(),
			);
			return;
		}
		self.print_line(&format!(
			"{} {}",
			self.paint("warning:", Style::Warning),
			message,
		));
	}

	pub(crate) fn command_started(&self) {
		// Guard: skip if disabled or already started
		if !self.enabled || self.command_started.swap(true, Ordering::Relaxed) {
			return;
		}

		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "command_started",
				"command": self.command_name,
				"dry_run": self.dry_run,
				"total_steps": self.total_steps,
			}));
			return;
		}

		let suffix = if self.dry_run { " (dry-run)" } else { "" };
		self.print_line(&format!(
			"{} {}{}",
			self.paint("monochange", Style::Accent),
			self.paint(&format!("running `{}`", self.command_name), Style::Header),
			suffix,
		));
	}

	pub(crate) fn command_finished(&self, duration: Duration) {
		if !self.enabled || !self.command_started.load(Ordering::Relaxed) {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "command_finished",
				"command": self.command_name,
				"dry_run": self.dry_run,
				"total_steps": self.total_steps,
				"duration_ms": duration_millis(duration),
			}));
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.command_success, Style::Success),
			self.paint(&format!("`{}` finished", self.command_name), Style::Header),
			self.paint(&format_duration(duration), Style::Muted),
		));
	}

	pub(crate) fn command_failed(&self, duration: Duration, error: &str) {
		if !self.enabled || !self.command_started.load(Ordering::Relaxed) {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "command_failed",
				"command": self.command_name,
				"dry_run": self.dry_run,
				"total_steps": self.total_steps,
				"duration_ms": duration_millis(duration),
				"error": error,
			}));
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_failure, Style::Error),
			self.paint(&format!("`{}` failed", self.command_name), Style::Header),
			self.paint(&format_duration(duration), Style::Muted),
		));
	}

	pub(crate) fn step_started(&self, step_index: usize, step: &CliStepDefinition) {
		if !self.enabled {
			return;
		}
		self.command_started();
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_step_event("step_started", step_index, step, serde_json::Map::new());
			return;
		}
		let message = self.step_message(step_index, step);
		if self.animate {
			self.start_spinner(message);
		} else {
			self.print_line(&format!(
				"{} {message}",
				self.paint(self.symbols.step_start, Style::Accent)
			));
		}
	}

	pub(crate) fn step_skipped(
		&self,
		step_index: usize,
		step: &CliStepDefinition,
		condition: Option<&str>,
		reason: Option<&str>,
	) {
		if !self.enabled {
			return;
		}
		self.command_started();
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.extend(
				condition.map(|condition| ("condition".to_string(), condition.to_string().into())),
			);
			payload.extend(reason.map(|reason| ("reason".to_string(), reason.to_string().into())));
			self.emit_step_event("step_skipped", step_index, step, payload);
			return;
		}
		let mut line = format!(
			"{} {} — {}",
			self.paint(self.symbols.step_skip, Style::Warning),
			self.step_message(step_index, step),
			self.paint("skipped", Style::Muted),
		);
		if let Some(detail) = reason.or(condition) {
			let _ = write!(
				line,
				" {}",
				self.paint(&format!("({detail})"), Style::Muted)
			);
		}
		self.print_line(&line);
	}

	pub(crate) fn step_status(&self, step_index: usize, step: &CliStepDefinition, status: &str) {
		if !self.enabled {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"status".to_string(),
				serde_json::Value::String(status.to_string()),
			);
			self.emit_step_event("step_status", step_index, step, payload);
			return;
		}
		let message = format!(
			"{} — {}",
			self.step_message(step_index, step),
			self.paint(status, Style::Detail),
		);
		if self.animate {
			self.start_spinner(message);
		} else {
			self.print_line(&format!(
				"{} {message}",
				self.paint(self.symbols.step_start, Style::Accent),
			));
		}
	}

	pub(crate) fn step_finished(
		&self,
		step_index: usize,
		step: &CliStepDefinition,
		duration: Duration,
		phase_timings: &[StepPhaseTiming],
	) {
		if !self.enabled {
			return;
		}
		self.command_started();
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"duration_ms".to_string(),
				serde_json::Value::from(duration_millis(duration)),
			);
			payload.insert(
				"phase_timings".to_string(),
				serde_json::to_value(
					phase_timings
						.iter()
						.map(|phase| {
							ProgressPhaseTiming {
								label: phase.label.clone(),
								duration_ms: duration_millis(phase.duration),
							}
						})
						.collect::<Vec<_>>(),
				)
				.unwrap_or_else(|error| panic!("progress phase timing serialization: {error}")),
			);
			self.emit_step_event("step_finished", step_index, step, payload);
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_success, Style::Success),
			self.step_message(step_index, step),
			self.paint(&format_duration(duration), Style::Muted),
		));
		for phase in summarized_phase_timings(phase_timings) {
			self.print_line(&format!(
				"  {} {} {}",
				self.paint(self.symbols.bullet, Style::Muted),
				self.paint(&phase.label, Style::Detail),
				self.paint(&format_duration(phase.duration), Style::Muted),
			));
		}
	}

	pub(crate) fn step_failed(
		&self,
		step_index: usize,
		step: &CliStepDefinition,
		duration: Duration,
		error: &str,
	) {
		if !self.enabled {
			return;
		}
		self.command_started();
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"duration_ms".to_string(),
				serde_json::Value::from(duration_millis(duration)),
			);
			payload.insert(
				"error".to_string(),
				serde_json::Value::String(error.to_string()),
			);
			self.emit_step_event("step_failed", step_index, step, payload);
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_failure, Style::Error),
			self.step_message(step_index, step),
			self.paint(&format_duration(duration), Style::Muted),
		));
		for (index, line) in error.lines().enumerate() {
			let branch = if index == 0 {
				self.symbols.error_branch
			} else {
				self.symbols.log_pipe
			};
			self.print_line(&format!(
				"  {} {}",
				self.paint(branch, Style::Error),
				self.paint(line, Style::Error),
			));
		}
	}

	pub(crate) fn log_command_output(
		&self,
		step_index: usize,
		step: &CliStepDefinition,
		stream: CommandStream,
		text: &str,
	) {
		if !self.enabled || text.is_empty() {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"stream".to_string(),
				serde_json::Value::String(match stream {
					CommandStream::Stdout => "stdout".to_string(),
					CommandStream::Stderr => "stderr".to_string(),
				}),
			);
			payload.insert(
				"text".to_string(),
				serde_json::Value::String(text.to_string()),
			);
			self.emit_step_event("command_output", step_index, step, payload);
			return;
		}
		self.command_started();
		let stream_label = match stream {
			CommandStream::Stdout => self.paint("stdout", Style::Muted),
			CommandStream::Stderr => self.paint("stderr", Style::Warning),
		};
		let step_label = step.display_name();
		for line in text.lines() {
			let line = if self.color {
				line.to_string()
			} else {
				strip_terminal_controls(line)
			};
			let reset = if self.color { "\u{1b}[0m" } else { "" };
			self.print_line(&format!(
				"  {} {} {line}{reset}",
				self.paint(self.symbols.log_pipe, Style::Muted),
				self.paint(&format!("{step_label} [{stream_label}]"), Style::Detail),
			));
		}
	}

	fn step_message(&self, step_index: usize, step: &CliStepDefinition) -> String {
		let name = step.display_name();
		let kind = step.kind_name();
		let detail = if name == kind {
			String::new()
		} else {
			format!(" {}", self.paint(&format!("({kind})"), Style::Muted))
		};
		format!(
			"{} {}{}",
			self.paint(
				&format!("[{}/{}]", step_index + 1, self.total_steps),
				Style::Muted,
			),
			self.paint(name, Style::Header),
			detail,
		)
	}

	fn start_spinner(&self, message: String) {
		self.stop_spinner();
		let stop = Arc::new(AtomicBool::new(false));
		let rendered = Arc::new(AtomicBool::new(false));
		let stop_flag = Arc::clone(&stop);
		let rendered_flag = Arc::clone(&rendered);
		let line_cleared = Arc::clone(&self.line_cleared);
		let stderr = self.stderr.clone();
		let color = self.color;
		let spinner_frames = self.symbols.spinner_frames;
		let handle = thread::spawn(move || {
			thread::sleep(SPINNER_DELAY);
			let mut full_line = true;
			for frame in spinner_frames.iter().copied().cycle() {
				if stop_flag.load(Ordering::Relaxed) {
					break;
				}
				let was_cleared = line_cleared.swap(false, Ordering::Relaxed);
				let tick = render_spinner_tick(frame, &message, color, full_line || was_cleared);
				full_line = false;
				stderr.write(tick.as_bytes());
				rendered_flag.store(true, Ordering::Relaxed);
				thread::sleep(SPINNER_TICK);
			}
		});
		self.active_spinner.lock().unwrap().replace(SpinnerState {
			stop,
			rendered,
			handle,
		});
	}

	fn stop_spinner(&self) {
		let spinner = self.active_spinner.lock().unwrap().take();
		let Some(spinner) = spinner else {
			return;
		};
		spinner.stop.store(true, Ordering::Relaxed);
		let _ = spinner.handle.join();
		if spinner.rendered.load(Ordering::Relaxed) {
			self.stderr.write(b"\r\x1b[2K\x1b[0m");
		}
	}

	/// Stop the active spinner without printing a completion line so that
	/// interactive prompts can take over the terminal. Returns whether a
	/// spinner was actually running; callers can use that to restart it
	/// with `step_status` once the interactive work is done.
	pub(crate) fn pause_spinner(&self) -> bool {
		let was_active = self.active_spinner.lock().unwrap().is_some();
		self.stop_spinner();
		was_active
	}

	fn print_line(&self, text: &str) {
		let spinner_active = self.animate && self.active_spinner.lock().unwrap().is_some();
		let prefix = if spinner_active {
			"\r\u{1b}[2K\u{1b}[0m"
		} else {
			""
		};
		self.stderr.write(format!("{prefix}{text}\n").as_bytes());
		if spinner_active {
			self.line_cleared.store(true, Ordering::Relaxed);
		}
	}

	fn paint(&self, text: &str, style: Style) -> String {
		paint_text(text, style, self.color)
	}

	fn next_sequence(&self) -> u64 {
		self.event_sequence.fetch_add(1, Ordering::Relaxed)
	}

	fn emit_step_event(
		&self,
		event: &str,
		step_index: usize,
		step: &CliStepDefinition,
		mut payload: serde_json::Map<String, serde_json::Value>,
	) {
		payload.insert(
			"sequence".to_string(),
			serde_json::Value::from(self.next_sequence()),
		);
		payload.insert(
			"event".to_string(),
			serde_json::Value::String(event.to_string()),
		);
		payload.insert(
			"command".to_string(),
			serde_json::Value::String(self.command_name.clone()),
		);
		payload.insert("dry_run".to_string(), serde_json::Value::Bool(self.dry_run));
		payload.insert(
			"step_index".to_string(),
			serde_json::Value::from(step_index + 1),
		);
		payload.insert(
			"total_steps".to_string(),
			serde_json::Value::from(self.total_steps),
		);
		payload.insert(
			"step_kind".to_string(),
			serde_json::Value::String(step.kind_name().to_string()),
		);
		payload.insert(
			"step_display_name".to_string(),
			serde_json::Value::String(step.display_name().to_string()),
		);
		payload.insert(
			"step_name".to_string(),
			step.name().map_or(serde_json::Value::Null, |name| {
				serde_json::Value::String(name.to_string())
			}),
		);
		self.emit_json_event(&serde_json::Value::Object(payload));
	}

	fn emit_json_event(&self, value: &serde_json::Value) {
		let line = serde_json::to_string(value)
			.unwrap_or_else(|error| panic!("progress json event serialization: {error}"));
		self.stderr.write(format!("{line}\n").as_bytes());
	}

	fn emit_domain_json_event(
		&self,
		event: &str,
		mut payload: serde_json::Map<String, serde_json::Value>,
	) {
		payload.insert(
			"sequence".to_string(),
			serde_json::Value::from(self.next_sequence()),
		);
		payload.insert(
			"event".to_string(),
			serde_json::Value::String(event.to_string()),
		);
		payload.insert(
			"command".to_string(),
			serde_json::Value::String(self.command_name.clone()),
		);
		payload.insert("dry_run".to_string(), serde_json::Value::Bool(self.dry_run));
		self.emit_json_event(&serde_json::Value::Object(payload));
	}
}

impl LintProgressReporter for ProgressReporter {
	fn planning_started(&self, suites: &[&str]) {
		if !self.enabled || suites.is_empty() {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_planning_started",
				serde_json::json!({ "suites": suites })
					.as_object()
					.cloned()
					.unwrap_or_default(),
			);
			return;
		}
		let message = format!(
			"Running {} lint suite{}",
			suites.len(),
			if suites.len() == 1 { "" } else { "s" },
		);
		self.print_line(&format!(
			"{} {}",
			self.paint(self.symbols.step_start, Style::Accent),
			self.paint(&message, Style::Header),
		));
	}

	fn planning_finished(&self, total_files: usize, total_rules: usize) {
		if self.render_mode == ProgressRenderMode::Json && self.enabled {
			self.emit_domain_json_event(
				"lint_planning_finished",
				serde_json::json!({
					"total_files": total_files,
					"total_rules": total_rules,
				})
				.as_object()
				.cloned()
				.unwrap_or_default(),
			);
		}
	}

	fn suite_started(&self, suite_id: &str, file_count: usize, rule_count: usize) {
		if !self.enabled {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_suite_started",
				serde_json::json!({
					"suite": suite_id,
					"file_count": file_count,
					"rule_count": rule_count,
				})
				.as_object()
				.cloned()
				.unwrap_or_default(),
			);
			return;
		}
		let message = format!(
			"{suite_id} — checking {file_count} file{} with {rule_count} rule{}",
			if file_count == 1 { "" } else { "s" },
			if rule_count == 1 { "" } else { "s" },
		);
		if self.animate {
			self.start_spinner(message);
		} else {
			self.print_line(&format!(
				"{} {message}",
				self.paint(self.symbols.step_start, Style::Accent),
			));
		}
	}

	fn suite_finished(&self, suite_id: &str, result_count: usize, fixable_count: usize) {
		if !self.enabled {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_suite_finished",
				serde_json::json!({
					"suite": suite_id,
					"result_count": result_count,
					"fixable_count": fixable_count,
				})
				.as_object()
				.cloned()
				.unwrap_or_default(),
			);
			return;
		}
		let fixable = if fixable_count > 0 {
			format!(" ({fixable_count} fixable)")
		} else {
			String::new()
		};
		self.print_line(&format!(
			"{} {suite_id} — {result_count} issue{}{fixable}",
			self.paint(self.symbols.step_success, Style::Success),
			if result_count == 1 { "" } else { "s" },
		));
	}

	fn file_started(&self, _file_path: &Path, _rule_count: usize) {}

	fn file_finished(&self, _file_path: &Path, _result_count: usize) {}

	fn file_rule_started(&self, _file_path: &Path, _rule_id: &str) {}

	fn file_rule_finished(&self, _file_path: &Path, _rule_id: &str, _result_count: usize) {}

	fn fix_started(&self, file_count: usize) {
		if !self.enabled {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_fix_started",
				serde_json::json!({ "file_count": file_count })
					.as_object()
					.cloned()
					.unwrap_or_default(),
			);
			return;
		}
		let message = format!(
			"Applying fixes to {file_count} file{}",
			if file_count == 1 { "" } else { "s" },
		);
		if self.animate {
			self.start_spinner(message);
		} else {
			self.print_line(&format!(
				"{} {message}",
				self.paint(self.symbols.step_start, Style::Accent),
			));
		}
	}

	fn fix_applied(&self, file_path: &Path, description: &str) {
		if !self.enabled {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_fix_applied",
				serde_json::json!({
					"path": file_path,
					"description": description,
				})
				.as_object()
				.cloned()
				.unwrap_or_default(),
			);
			return;
		}
		self.print_line(&format!(
			"  {} {} ({description})",
			self.paint(self.symbols.bullet, Style::Success),
			file_path.display(),
		));
	}

	fn fix_finished(&self, files_fixed: usize) {
		if !self.enabled {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_fix_finished",
				serde_json::json!({ "files_fixed": files_fixed })
					.as_object()
					.cloned()
					.unwrap_or_default(),
			);
			return;
		}
		self.print_line(&format!(
			"{} Fixed {files_fixed} file{}",
			self.paint(self.symbols.step_success, Style::Success),
			if files_fixed == 1 { "" } else { "s" },
		));
	}

	fn summary(&self, errors: usize, warnings: usize, fixable: usize, fixed: bool) {
		if !self.enabled || errors == 0 && warnings == 0 {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_domain_json_event(
				"lint_summary",
				serde_json::json!({
					"errors": errors,
					"warnings": warnings,
					"fixable": fixable,
					"fixed": fixed,
				})
				.as_object()
				.cloned()
				.unwrap_or_default(),
			);
			return;
		}
		self.print_line(&self.paint("─────────────────────────────", Style::Muted));
		if let Some(line) = summary_count_line(errors, warnings, self.symbols.step_failure, "!") {
			self.print_line(&line);
		}
		if fixable > 0 {
			let verb = if fixed {
				"remain auto-fixable"
			} else {
				"can be auto-fixed"
			};
			let suffix = if fixed { " again" } else { "" };
			self.print_line(&format!(
				"{} {fixable} issue{} {verb}. Run `monochange check --fix`{suffix} to apply.",
				self.paint(self.symbols.bullet, Style::Muted),
				if fixable == 1 { "" } else { "s" },
			));
		}
	}
}

impl PublishProgressReporter for ProgressReporter {
	fn report(&self, event: PublishProgressEvent) {
		if !self.enabled {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			let (event_name, payload) = publish_event_json(&event);
			self.emit_domain_json_event(event_name, payload);
			return;
		}
		let activity = matches!(
			event,
			PublishProgressEvent::RegistryCheckStarted(_) | PublishProgressEvent::PackageStarted(_)
		);
		let line = render_publish_event(
			&event,
			self.capabilities.stderr_is_terminal && !self.capabilities.ci,
		);
		if activity && self.animate {
			self.start_spinner(line);
		} else {
			if !activity {
				self.stop_spinner();
			}
			self.print_line(&line);
		}
	}
}

fn summary_count_line(
	errors: usize,
	warnings: usize,
	error_icon: &str,
	warning_icon: &str,
) -> Option<String> {
	let mut line = String::new();
	if errors > 0 {
		let _ = write!(
			line,
			"{error_icon} {errors} error{}",
			if errors == 1 { "" } else { "s" },
		);
	}
	if warnings > 0 {
		if !line.is_empty() {
			line.push_str(", ");
		}
		let _ = write!(
			line,
			"{warning_icon} {warnings} warning{}",
			if warnings == 1 { "" } else { "s" },
		);
	}
	(!line.is_empty()).then_some(line)
}

fn render_publish_event(event: &PublishProgressEvent, interactive: bool) -> String {
	let mut output = String::with_capacity(128);
	match event {
		PublishProgressEvent::RunStarted {
			mode,
			dry_run,
			total,
			ecosystems,
		} => {
			let dry_run = if *dry_run { " dry-run" } else { "" };
			let _ = write!(output, "◆ Publishing {total} packages ({mode:?}{dry_run})");
			if !ecosystems.is_empty() {
				output.push_str(" across ");
				append_ecosystems(&mut output, ecosystems);
			}
		}
		PublishProgressEvent::RegistryCheckStarted(package) => {
			output.push_str(start_symbol(interactive));
			output.push(' ');
			append_package_prefix(&mut output, package);
			let _ = write!(
				output,
				" checking {} on {}",
				package.version, package.registry,
			);
		}
		PublishProgressEvent::PackageStarted(package) => {
			output.push_str(start_symbol(interactive));
			output.push(' ');
			append_package_prefix(&mut output, package);
			let _ = write!(
				output,
				" publishing {} to {}",
				package.version, package.registry,
			);
		}
		PublishProgressEvent::PackageSkipped { package, message } => {
			output.push_str("⏭️ ");
			append_package_prefix(&mut output, package);
			let _ = write!(output, " {message}");
		}
		PublishProgressEvent::PackagePlanned(package) => {
			output.push_str("📝 ");
			append_package_prefix(&mut output, package);
			let _ = write!(
				output,
				" would publish {} to {}",
				package.version, package.registry,
			);
		}
		PublishProgressEvent::PackagePublished(package) => {
			output.push_str("✅ ");
			append_package_prefix(&mut output, package);
			let _ = write!(
				output,
				" published {} to {}",
				package.version, package.registry,
			);
		}
		PublishProgressEvent::PackageFailed { package, message } => {
			output.push_str("❌ ");
			append_package_prefix(&mut output, package);
			let _ = write!(output, " failed: {message}");
		}
		PublishProgressEvent::RunFinished {
			total,
			published,
			skipped,
			failed,
			..
		} => {
			let _ = write!(
				output,
				"◆ Publish complete: {total} expected, ✅ {published} succeeded, ❌ {failed} failed, ⏭️ {skipped} skipped",
			);
		}
	}
	output
}

fn publish_event_json(
	event: &PublishProgressEvent,
) -> (&'static str, serde_json::Map<String, serde_json::Value>) {
	let (name, value) = match event {
		PublishProgressEvent::RunStarted {
			mode,
			dry_run,
			total,
			ecosystems,
		} => {
			(
				"publish_run_started",
				serde_json::json!({
					"mode": mode,
					"publish_dry_run": dry_run,
					"total": total,
					"ecosystems": ecosystems,
				}),
			)
		}
		PublishProgressEvent::RegistryCheckStarted(package) => {
			(
				"publish_registry_check_started",
				serde_json::json!({ "package": publish_package_json(package) }),
			)
		}
		PublishProgressEvent::PackageStarted(package) => {
			(
				"publish_package_started",
				serde_json::json!({ "package": publish_package_json(package) }),
			)
		}
		PublishProgressEvent::PackageSkipped { package, message } => {
			(
				"publish_package_skipped",
				serde_json::json!({
					"package": publish_package_json(package),
					"message": message,
				}),
			)
		}
		PublishProgressEvent::PackagePlanned(package) => {
			(
				"publish_package_planned",
				serde_json::json!({ "package": publish_package_json(package) }),
			)
		}
		PublishProgressEvent::PackagePublished(package) => {
			(
				"publish_package_published",
				serde_json::json!({ "package": publish_package_json(package) }),
			)
		}
		PublishProgressEvent::PackageFailed { package, message } => {
			(
				"publish_package_failed",
				serde_json::json!({
					"package": publish_package_json(package),
					"message": message,
				}),
			)
		}
		PublishProgressEvent::RunFinished {
			mode,
			total,
			published,
			skipped,
			failed,
		} => {
			(
				"publish_run_finished",
				serde_json::json!({
					"mode": mode,
					"total": total,
					"published": published,
					"skipped": skipped,
					"failed": failed,
				}),
			)
		}
	};
	(name, value.as_object().cloned().unwrap_or_default())
}

fn publish_package_json(package: &PublishProgressPackage) -> serde_json::Value {
	serde_json::json!({
		"package_id": package.package_id,
		"package_name": package.package_name,
		"version": package.version,
		"ecosystem": package.ecosystem,
		"registry": package.registry,
	})
}

fn start_symbol(interactive: bool) -> &'static str {
	if interactive { "⠋" } else { "→" }
}

fn append_ecosystems(output: &mut String, ecosystems: &[monochange_core::Ecosystem]) {
	for (index, ecosystem) in ecosystems.iter().enumerate() {
		if index > 0 {
			output.push_str(", ");
		}
		output.push_str(ecosystem.progress_emoji());
		output.push(' ');
		output.push_str(ecosystem.progress_label());
	}
}

fn append_package_prefix(output: &mut String, package: &PublishProgressPackage) {
	output.push_str(package.ecosystem.progress_emoji());
	output.push(' ');
	output.push_str(package.ecosystem.progress_label());
	output.push(' ');
	output.push_str(&package.package_name);
}

impl Drop for ProgressReporter {
	fn drop(&mut self) {
		self.stop_spinner();
	}
}

#[derive(Clone, Copy)]
enum Style {
	Accent,
	Success,
	Warning,
	Error,
	Header,
	Detail,
	Muted,
}

fn paint_text(text: &str, style: Style, color: bool) -> String {
	if !color {
		return text.to_string();
	}
	let code = match style {
		Style::Accent => "36;1",
		Style::Success => "32;1",
		Style::Warning => "33;1",
		Style::Error => "31;1",
		Style::Header => "37;1",
		Style::Detail => "35",
		Style::Muted => "2",
	};
	format!("\u{1b}[{code}m{text}\u{1b}[0m")
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
		match characters.next() {
			Some('[') => {
				for control in characters.by_ref() {
					if ('@'..='~').contains(&control) {
						break;
					}
				}
			}
			Some(']') => {
				let mut previous_was_escape = false;
				for control in characters.by_ref() {
					if control == '\u{7}' || previous_was_escape && control == '\\' {
						break;
					}
					previous_was_escape = control == '\u{1b}';
				}
			}
			Some(_) | None => {}
		}
	}
	output
}

/// Renders a single spinner tick. The full line (with erase) is only written
/// when the message must be (re)established — the first tick or after another
/// writer cleared the line. Otherwise only the frame is swapped in place so
/// captured output does not reprint the whole line on every tick.
fn render_spinner_tick(frame: &str, message: &str, color: bool, full_line: bool) -> String {
	if full_line {
		format!(
			"\r\u{1b}[2K\u{1b}[0m{} {}",
			paint_text(frame, Style::Accent, color),
			message,
		)
	} else {
		format!("\r{}", paint_text(frame, Style::Accent, color))
	}
}

fn format_duration(duration: Duration) -> String {
	if duration >= Duration::from_secs(60) {
		let seconds = duration.as_secs_f64();
		return format!("{seconds:.1}s");
	}
	if duration >= Duration::from_secs(1) {
		let seconds = duration.as_secs_f64();
		return format!("{seconds:.2}s");
	}
	if duration >= Duration::from_millis(1) {
		return format!("{}ms", duration.as_millis());
	}
	format!("{}µs", duration.as_micros())
}

fn duration_millis(duration: Duration) -> u64 {
	u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn summarized_phase_timings(phase_timings: &[StepPhaseTiming]) -> Vec<StepPhaseTiming> {
	let mut phase_timings = phase_timings
		.iter()
		.filter(|phase| phase.duration >= PHASE_TIMING_MINIMUM)
		.cloned()
		.collect::<Vec<_>>();
	phase_timings.sort_by_key(|phase| Reverse(phase.duration));
	phase_timings.truncate(PHASE_TIMING_DETAIL_LIMIT);
	phase_timings
}

#[cfg(test)]
#[path = "../__tests__/cli_progress_tests.rs"]
mod tests;
