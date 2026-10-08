#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::Ecosystem;
use monochange_core::ShellConfig;
use monochange_core::lint::LintProgressReporter;
use monochange_publish::PackagePublishRunMode;
use monochange_publish::PublishProgressEvent;
use monochange_publish::PublishProgressPackage;
use monochange_publish::PublishProgressReporter;
use temp_env::with_var;

use super::*;

fn progress_reporter(enabled: bool, color: bool) -> ProgressReporter {
	ProgressReporter::with_context(
		"release".to_string(),
		false,
		3,
		ProgressFormat::Unicode,
		TerminalCapabilities {
			stdout_is_terminal: true,
			stderr_is_terminal: true,
			ci: false,
			quiet: !enabled,
			color,
			animate: false,
			progress_enabled: enabled,
			github_actions: false,
			verbose: false,
		},
		SharedStderr::with_writer(io::sink()),
	)
}

fn named_command_step(name: &str) -> CliStepDefinition {
	CliStepDefinition::Command {
		show_progress: None,
		name: Some(name.to_string()),
		when: None,
		always_run: false,
		command: "echo hi".to_string(),
		dry_run_command: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	}
}

fn command_with_step(step: CliStepDefinition) -> CliCommandDefinition {
	CliCommandDefinition {
		name: "release".to_string(),
		help_text: Some("release".to_string()),
		inputs: Vec::new(),
		steps: vec![step],
		dry_run: false,
	}
}

#[derive(Clone)]
struct RecordedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for RecordedWriter {
	fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
		self.0.lock().unwrap().extend_from_slice(bytes);
		Ok(bytes.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

fn recorded_reporter(format: ProgressFormat) -> (ProgressReporter, Arc<Mutex<Vec<u8>>>) {
	let bytes = Arc::new(Mutex::new(Vec::new()));
	let reporter = ProgressReporter::with_context(
		"release".to_string(),
		false,
		1,
		format,
		TerminalCapabilities {
			stdout_is_terminal: false,
			stderr_is_terminal: false,
			ci: true,
			quiet: false,
			color: false,
			animate: false,
			progress_enabled: true,
			github_actions: false,
			verbose: false,
		},
		SharedStderr::with_writer(RecordedWriter(Arc::clone(&bytes))),
	);
	(reporter, bytes)
}

fn capabilities(animate: bool, github_actions: bool) -> TerminalCapabilities {
	TerminalCapabilities {
		stdout_is_terminal: false,
		stderr_is_terminal: false,
		ci: true,
		quiet: false,
		color: false,
		animate,
		progress_enabled: true,
		github_actions,
		verbose: false,
	}
}

fn recorded_reporter_with(
	total_steps: usize,
	capabilities: TerminalCapabilities,
	format: ProgressFormat,
) -> (ProgressReporter, Arc<Mutex<Vec<u8>>>) {
	let bytes = Arc::new(Mutex::new(Vec::new()));
	let reporter = ProgressReporter::with_context(
		"release".to_string(),
		false,
		total_steps,
		format,
		capabilities,
		SharedStderr::with_writer(RecordedWriter(Arc::clone(&bytes))),
	);
	(reporter, bytes)
}

fn recorded_text(bytes: &Arc<Mutex<Vec<u8>>>) -> String {
	String::from_utf8(bytes.lock().unwrap().clone()).unwrap()
}

fn builtin_step(name: Option<&str>) -> CliStepDefinition {
	CliStepDefinition::Discover {
		name: name.map(ToString::to_string),
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	}
}

fn multi_step_command() -> CliCommandDefinition {
	CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			builtin_step(Some("plan release")),
			named_command_step("format files"),
		],
		dry_run: true,
	}
}

fn publish_package() -> PublishProgressPackage {
	PublishProgressPackage {
		package_id: "cli".to_string(),
		package_name: "monochange".to_string(),
		version: "1.2.3".to_string(),
		ecosystem: Ecosystem::Cargo,
		registry: "crates.io".to_string(),
	}
}

#[test]
fn format_duration_and_paint_text_cover_terminal_styles() {
	assert_eq!(paint_text("plain", Style::Detail, false), "plain");
	assert_eq!(
		paint_text("accent", Style::Accent, true),
		"\u{1b}[36;1maccent\u{1b}[0m"
	);
	assert_eq!(
		paint_text("success", Style::Success, true),
		"\u{1b}[32;1msuccess\u{1b}[0m"
	);
	assert_eq!(
		paint_text("warn", Style::Warning, true),
		"\u{1b}[33;1mwarn\u{1b}[0m"
	);
	assert_eq!(
		paint_text("error", Style::Error, true),
		"\u{1b}[31;1merror\u{1b}[0m"
	);
	assert_eq!(
		paint_text("detail", Style::Detail, true),
		"\u{1b}[35mdetail\u{1b}[0m"
	);
	assert_eq!(
		paint_text("header", Style::Header, true),
		"\u{1b}[37;1mheader\u{1b}[0m"
	);
	assert_eq!(
		paint_text("muted", Style::Muted, true),
		"\u{1b}[2mmuted\u{1b}[0m"
	);
	assert_eq!(format_duration(Duration::from_secs(3_720)), "1h 02m");
	assert_eq!(format_duration(Duration::from_secs(456)), "7m 36s");
	assert_eq!(format_duration(Duration::from_millis(58_750)), "58.8s");
	assert_eq!(format_duration(Duration::from_millis(1500)), "1.50s");
	assert_eq!(format_duration(Duration::from_millis(937)), "937ms");
	assert_eq!(format_duration(Duration::from_micros(12)), "12µs");
}

#[test]
fn progress_format_parsing_and_renderer_selection_cover_all_variants() {
	assert_eq!(ProgressFormat::parse("auto"), Some(ProgressFormat::Auto));
	assert_eq!(
		ProgressFormat::parse("unicode"),
		Some(ProgressFormat::Unicode)
	);
	assert_eq!(ProgressFormat::parse("ascii"), Some(ProgressFormat::Ascii));
	assert_eq!(ProgressFormat::parse("json"), Some(ProgressFormat::Json));
	assert_eq!(ProgressFormat::parse("wat"), None);

	let (unicode, _) = recorded_reporter(ProgressFormat::Unicode);
	assert!(unicode.enabled);
	assert_eq!(unicode.render_mode, ProgressRenderMode::Human);
	assert_eq!(
		unicode.symbols.command_success,
		UNICODE_SYMBOLS.command_success
	);

	let (ascii, _) = recorded_reporter(ProgressFormat::Ascii);
	assert!(ascii.enabled);
	assert_eq!(ascii.render_mode, ProgressRenderMode::Human);
	assert_eq!(ascii.symbols.command_success, ASCII_SYMBOLS.command_success);

	let (auto, _) = recorded_reporter(ProgressFormat::Auto);
	assert!(auto.enabled);

	let quiet = progress_reporter(false, false);
	assert!(!quiet.enabled);

	let (json, _) = recorded_reporter(ProgressFormat::Json);
	assert!(json.enabled);
	assert_eq!(json.render_mode, ProgressRenderMode::Json);
	assert_eq!(json.symbols.command_success, ASCII_SYMBOLS.command_success);
}

#[test]
fn every_progress_format_respects_the_progress_opt_out() {
	let command = command_with_step(named_command_step("announce release"));

	with_var("MONOCHANGE_NO_PROGRESS", Some("1"), || {
		for format in [
			ProgressFormat::Auto,
			ProgressFormat::Unicode,
			ProgressFormat::Ascii,
			ProgressFormat::Json,
		] {
			let reporter = ProgressReporter::new(&command, false, false, format);
			assert!(!reporter.enabled);
		}
	});
}

#[test]
fn progress_reporter_renders_skips_failures_and_stderr_output_when_enabled() {
	let reporter = progress_reporter(true, false);
	let step = named_command_step("announce release");

	reporter.step_skipped(0, &step, None, None);
	reporter.step_skipped(0, &step, Some("{{ false }}"), Some("condition is false"));
	reporter.log_command_output(0, &step, CommandStream::Stderr, "warn line\n");
	reporter.step_failed(1, &step, Duration::from_millis(25), "boom\nagain");
	reporter.command_failed(Duration::from_millis(30), "boom");
}

#[test]
fn progress_reporter_emits_json_skip_and_failure_events() {
	let mut reporter = progress_reporter(true, false);
	reporter.render_mode = ProgressRenderMode::Json;
	let step = named_command_step("announce release");

	reporter.step_skipped(0, &step, Some("{{ false }}"), Some("condition is false"));
	reporter.step_failed(1, &step, Duration::from_millis(25), "boom");
	reporter.command_failed(Duration::from_millis(30), "boom");
}

#[test]
fn progress_reporter_updates_step_status_in_human_json_and_animated_modes() {
	let step = named_command_step("retarget release");
	let disabled = progress_reporter(false, false);
	disabled.step_status(0, &step, "locating release record");

	let human = progress_reporter(true, false);
	human.step_status(0, &step, "planning retarget");

	let mut json = progress_reporter(true, false);
	json.render_mode = ProgressRenderMode::Json;
	json.step_status(0, &step, "applying git ref and provider updates");
	assert_eq!(json.stderr.next_sequence(), 1);

	let mut animated = progress_reporter(true, true);
	animated.animate = true;
	animated.step_status(0, &step, "syncing provider metadata");
	assert!(animated.active_spinner.lock().unwrap().is_some());
	animated.stop_spinner();
}

#[test]
fn progress_reporter_animates_named_steps_and_stops_cleanly() {
	let mut reporter = progress_reporter(true, true);
	reporter.animate = true;
	let step = named_command_step("announce release");

	reporter.command_started();
	reporter.step_started(0, &step);
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(20));
	reporter.step_finished(
		0,
		&step,
		Duration::from_millis(12),
		&[StepPhaseTiming {
			label: "build release plan".to_string(),
			duration: Duration::from_millis(8),
		}],
	);
	reporter.command_finished(Duration::from_millis(25));
}

#[test]
fn pause_spinner_stops_animation_and_reports_whether_it_was_active() {
	let mut reporter = progress_reporter(true, true);
	reporter.animate = true;
	let step = named_command_step("announce release");

	// No active spinner: pause reports false and is a no-op.
	assert!(!reporter.pause_spinner());

	reporter.step_started(0, &step);
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(20));
	assert!(reporter.pause_spinner());

	// The spinner thread is stopped; a second pause reports false.
	assert!(!reporter.pause_spinner());
}

#[test]
fn spinner_tick_renders_full_line_only_when_content_changes() {
	// First tick (or after another writer cleared the line): full line with
	// erase so the message is always visible.
	assert_eq!(
		render_spinner_tick("\u{2B8B}", "running command `x`", false, true),
		"\r\u{1b}[2K\u{1b}[0m\u{2B8B} running command `x`",
	);
	// Unchanged content: only the frame is swapped in place, the message is
	// not reprinted.
	assert_eq!(
		render_spinner_tick("\u{2B99}", "running command `x`", false, false),
		"\r\u{2B99}",
	);
	// Color mode paints the frame.
	assert_eq!(
		render_spinner_tick("\u{2B8B}", "msg", true, true),
		"\r\u{1b}[2K\u{1b}[0m\u{1b}[36;1m\u{2B8B}\u{1b}[0m msg",
	);
	assert_eq!(
		render_spinner_tick("\u{2B99}", "msg", true, false),
		"\r\u{1b}[36;1m\u{2B99}\u{1b}[0m",
	);
}

#[test]
fn spinner_rewrites_full_line_after_another_writer_clears_it() {
	let mut reporter = progress_reporter(true, true);
	reporter.animate = true;
	let step = named_command_step("announce release");

	reporter.step_started(0, &step);
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(20));
	// Another writer (for example a warning or publish progress) clears the
	// spinner line: the next tick must restore the full line.
	reporter.stderr.write_line("another writer");
	thread::sleep(SPINNER_TICK + Duration::from_millis(20));
	reporter.step_finished(0, &step, Duration::from_millis(12), &[]);
	reporter.command_finished(Duration::from_millis(25));
}

#[test]
fn redirected_subprocess_output_strips_terminal_controls() {
	assert_eq!(
		strip_terminal_controls("\x1b[33mwarning\x1b[0m\r"),
		"warning"
	);
	assert_eq!(
		strip_terminal_controls("\x1b]0;secret title\x07visible"),
		"visible"
	);
}

#[test]
fn captured_command_output_streams_under_the_step_without_repeating_it() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Auto);
	let step = named_command_step("format release files");
	reporter.log_command_output(
		0,
		&step,
		CommandStream::Stdout,
		"line one\n\nansi \u{1b}[33mwarning\u{1b}[0m\n",
	);
	reporter.log_command_output(0, &step, CommandStream::Stderr, "warn line\n");
	reporter.step_status(0, &step, "still running");
	reporter.log_command_output(0, &step, CommandStream::Stdout, "line two");

	assert_eq!(
		recorded_text(&bytes),
		concat!(
			"  │ line one\n",
			"  │\n",
			"  │ ansi warning\n",
			"  │ warn line\n",
			"  › still running\n",
			"  │ line two\n",
		)
	);
}

#[test]
fn animated_command_output_names_its_step_once_per_block() {
	let (reporter, bytes) =
		recorded_reporter_with(2, capabilities(true, false), ProgressFormat::Unicode);
	let step = named_command_step("format release files");
	reporter.log_command_output(0, &step, CommandStream::Stdout, "line one\n");
	reporter.log_command_output(0, &step, CommandStream::Stderr, "line two\n");
	reporter.warning("interrupting line");
	reporter.log_command_output(0, &step, CommandStream::Stdout, "line three");

	assert_eq!(
		recorded_text(&bytes),
		concat!(
			"monochange › release · 2 steps\n",
			"  │ format release files\n",
			"  │   line one\n",
			"  │   line two\n",
			"warning: interrupting line\n",
			"  │ format release files\n",
			"  │   line three\n",
		)
	);
}

#[test]
fn github_actions_folds_command_output_and_annotates_warnings_and_failures() {
	let (reporter, bytes) =
		recorded_reporter_with(2, capabilities(false, true), ProgressFormat::Unicode);
	let step = named_command_step("format files");
	reporter.step_started(1, &step);
	reporter.step_command(1, &step, "dprint fmt");
	reporter.log_command_output(1, &step, CommandStream::Stdout, "::set-output name=x::y\n");
	reporter.log_command_output(1, &step, CommandStream::Stdout, "formatted 3 files\n");
	reporter.warning("100% of files changed\nsecond line");
	reporter.step_failed(
		1,
		&step,
		Duration::from_millis(40),
		"discovery error: command `dprint fmt` failed: exit status: 1\nstderr:\nboom",
	);
	reporter.write_diagnostic(CliDiagnostic::from_error(
		&monochange_core::MonochangeError::Discovery(
			"command `dprint fmt` failed: exit status: 1".to_string(),
		),
		Some("monochange run release"),
	));

	insta::assert_snapshot!(recorded_text(&bytes));
}

#[test]
fn multi_step_commands_frame_their_steps_with_a_banner_and_summary() {
	let command = multi_step_command();
	let (mut reporter, bytes) =
		recorded_reporter_with(0, capabilities(false, false), ProgressFormat::Unicode);
	reporter.configure_command(&command, true);
	let [plan, format] = command.steps.as_slice() else {
		panic!("expected two steps");
	};
	reporter.step_started(0, plan);
	reporter.step_finished(
		0,
		plan,
		Duration::from_millis(1_250),
		&[
			StepPhaseTiming {
				label: "build release plan".to_string(),
				duration: Duration::from_millis(900),
			},
			StepPhaseTiming {
				label: "tiny".to_string(),
				duration: Duration::from_millis(1),
			},
			StepPhaseTiming {
				label: "resolve changelogs".to_string(),
				duration: Duration::from_millis(300),
			},
		],
	);
	reporter.step_started(1, format);
	reporter.step_command(1, format, "dprint fmt");
	reporter.step_skipped(1, format, None, Some("dry-run; the command was not run"));
	reporter.command_finished(Duration::from_secs(75));

	let (failing, failing_bytes) =
		recorded_reporter_with(2, capabilities(false, false), ProgressFormat::Ascii);
	failing.step_failed(0, plan, Duration::from_millis(5), "config error: bad input");
	failing.step_failed(1, format, Duration::from_millis(5), "\n");
	failing.command_failed(Duration::from_millis(12), "config error: bad input");

	insta::assert_snapshot!(format!(
		"{}---\n{}",
		recorded_text(&bytes),
		recorded_text(&failing_bytes)
	));
}

#[test]
fn single_step_commands_skip_the_command_banner() {
	let (reporter, bytes) =
		recorded_reporter_with(1, capabilities(false, false), ProgressFormat::Unicode);
	let step = builtin_step(None);
	reporter.step_started(0, &step);
	reporter.step_finished(0, &step, Duration::from_millis(3), &[]);
	reporter.command_finished(Duration::from_millis(4));
	reporter.command_failed(Duration::from_millis(4), "ignored");

	assert_eq!(
		recorded_text(&bytes),
		"▶ discover packages\n✔ discover packages  3ms\n"
	);
}

#[test]
fn heartbeats_follow_a_slow_schedule_in_captured_output() {
	let (reporter, bytes) =
		recorded_reporter_with(2, capabilities(false, false), ProgressFormat::Unicode);
	let step = named_command_step("compile workspace");
	reporter.step_started(0, &step);
	for seconds in [5, 29, 30, 35, 59, 61, 119, 125] {
		reporter.step_heartbeat(0, &step, Duration::from_secs(seconds));
	}

	assert_eq!(
		recorded_text(&bytes),
		concat!(
			"monochange › release · 2 steps\n",
			"▶ [1/2] compile workspace\n",
			"  … compile workspace still running · 30.0s\n",
			"  … compile workspace still running · 1m 01s\n",
			"  … compile workspace still running · 2m 05s\n",
		)
	);
	assert_eq!(
		next_heartbeat_after(Duration::from_secs(30)),
		Duration::from_secs(60)
	);
	assert_eq!(
		next_heartbeat_after(Duration::from_secs(130)),
		Duration::from_secs(180)
	);

	let (json, json_bytes) = recorded_reporter(ProgressFormat::Json);
	json.step_heartbeat(0, &step, Duration::from_secs(5));
	json.step_command(0, &step, "cargo build");
	let events = recorded_text(&json_bytes)
		.lines()
		.map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
		.collect::<Vec<_>>();
	assert_eq!(
		events
			.iter()
			.map(|event| event["status"].as_str().unwrap_or_default())
			.collect::<Vec<_>>(),
		[
			"still running external command after 5.0s",
			"running command `cargo build`",
		]
	);

	let (animated, animated_bytes) =
		recorded_reporter_with(1, capabilities(true, false), ProgressFormat::Unicode);
	animated.step_heartbeat(0, &step, Duration::from_secs(90));
	assert!(recorded_text(&animated_bytes).is_empty());

	let disabled = progress_reporter(false, false);
	disabled.step_heartbeat(0, &step, Duration::from_secs(90));
	disabled.step_command(0, &step, "ignored");
}

#[test]
fn animated_steps_show_the_command_and_elapsed_time_on_the_spinner() {
	let (reporter, bytes) =
		recorded_reporter_with(1, capabilities(true, false), ProgressFormat::Unicode);
	let step = named_command_step("compile workspace");
	reporter.step_started(0, &step);
	reporter.step_command(0, &step, "cargo build");
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(20));
	reporter.step_finished(0, &step, Duration::from_millis(12), &[]);

	let output = recorded_text(&bytes);
	assert!(
		output.contains("⠋ compile workspace · cargo build"),
		"{output}"
	);
	assert_eq!(spinner_elapsed(Duration::from_millis(1_900)), None);
	assert_eq!(
		spinner_elapsed(Duration::from_millis(5_400)),
		Some("5s".to_string())
	);
	assert_eq!(
		spinner_elapsed(Duration::from_secs(65)),
		Some("1m 05s".to_string())
	);
}

#[test]
fn step_labels_humanize_unnamed_built_in_steps() {
	assert_eq!(step_label(&builtin_step(None)), "discover packages");
	assert_eq!(step_label(&builtin_step(Some("scan"))), "scan");
	assert_eq!(
		step_kind_suffix(&builtin_step(Some("scan"))),
		Some("Discover")
	);
	assert_eq!(step_kind_suffix(&builtin_step(Some("Discover"))), None);
	assert_eq!(step_kind_suffix(&builtin_step(None)), None);
	assert_eq!(step_kind_suffix(&named_command_step("build")), None);
	for (kind, label) in [
		("Config", "resolve configuration"),
		("Validate", "validate workspace"),
		("Discover", "discover packages"),
		("DisplayVersions", "calculate next versions"),
		("CreateChangeFile", "create change file"),
		("PrepareRelease", "prepare release"),
		("CommitRelease", "commit release"),
		("VerifyReleaseBranch", "verify release branch"),
		("PublishRelease", "publish release"),
		("PlaceholderPublish", "publish placeholder packages"),
		("PublishPackages", "publish packages"),
		("PlanPublishRateLimits", "plan publish rate limits"),
		("OpenReleaseRequest", "open release request"),
		("CommentReleasedIssues", "comment on released issues"),
		("AffectedPackages", "check affected packages"),
		("DiagnoseChangesets", "diagnose changesets"),
		("ReleaseRecord", "read release record"),
		("PublishReadiness", "check publish readiness"),
		("TagRelease", "tag release"),
		("RetargetRelease", "retarget release"),
		("Command", "run command"),
	] {
		assert_eq!(humanized_step_kind(kind), label);
	}
}

#[test]
fn step_failure_lines_name_the_failure_without_internal_prefixes() {
	assert_eq!(
		error_summary("discovery error: command `make` failed: exit status: 2\nstderr:\nx"),
		Some("command failed (exit status: 2)".to_string())
	);
	assert_eq!(
		error_summary("\nio error: disk full"),
		Some("disk full".to_string())
	);
	assert_eq!(
		error_summary("plain failure"),
		Some("plain failure".to_string())
	);
	assert_eq!(error_summary("  \n "), None);
}

#[test]
fn warning_sinks_follow_the_progress_mode() {
	assert!(progress_reporter(false, false).warning_sink().is_none());
	let (human, _) = recorded_reporter(ProgressFormat::Auto);
	assert!(human.warning_sink().is_some());
	let (json, _) = recorded_reporter(ProgressFormat::Json);
	assert!(json.warning_sink().is_some());
	let (github, _) = recorded_reporter_with(1, capabilities(false, true), ProgressFormat::Auto);
	assert!(github.warning_sink().is_some());
}

#[test]
fn dropping_a_reporter_closes_an_open_github_group() {
	let (reporter, bytes) =
		recorded_reporter_with(1, capabilities(false, true), ProgressFormat::Unicode);
	let step = named_command_step("compile");
	reporter.log_command_output(0, &step, CommandStream::Stdout, "building\n");
	drop(reporter);

	assert_eq!(
		recorded_text(&bytes),
		"::group::compile · output\n  │ building\n::endgroup::\n"
	);
}

#[test]
fn disabled_progress_still_names_the_failed_step_in_the_diagnostic() {
	let (mut reporter, bytes) =
		recorded_reporter_with(2, capabilities(false, false), ProgressFormat::Auto);
	reporter.enabled = false;
	let step = named_command_step("run tests");
	reporter.step_failed(0, &step, Duration::from_millis(1), "boom");
	reporter.step_failed(1, &named_command_step("cleanup"), Duration::ZERO, "boom");
	reporter.write_diagnostic(CliDiagnostic::from_error(
		&monochange_core::MonochangeError::Io("boom".to_string()),
		Some("monochange run test"),
	));

	assert_eq!(
		recorded_text(&bytes),
		"error[io.failed]: boom\n  command: monochange run test\n  step:    [1/2] run tests\n  help:    Check that the path exists and is writable, then rerun the command.\n"
	);

	let (single, single_bytes) = recorded_reporter(ProgressFormat::Auto);
	single.step_failed(0, &step, Duration::from_millis(1), "boom");
	single.write_diagnostic(CliDiagnostic::from_error(
		&monochange_core::MonochangeError::Diagnostic("boom".to_string()),
		Some("monochange run test"),
	));
	assert!(
		recorded_text(&single_bytes)
			.ends_with("error[cli.diagnostic]: boom\n  command: monochange run test\n")
	);
}

#[test]
fn nested_workflow_and_publish_events_share_complete_lines() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Auto);
	let step = named_command_step("publish packages");
	reporter.command_started();
	reporter.step_started(0, &step);
	PublishProgressReporter::report(
		&reporter,
		PublishProgressEvent::RunStarted {
			mode: PackagePublishRunMode::Release,
			dry_run: true,
			total: 1,
			ecosystems: vec![Ecosystem::Cargo],
		},
	);
	PublishProgressReporter::report(
		&reporter,
		PublishProgressEvent::PackageStarted(publish_package()),
	);
	PublishProgressReporter::report(
		&reporter,
		PublishProgressEvent::PackagePlanned(publish_package()),
	);
	reporter.step_finished(
		0,
		&step,
		Duration::from_millis(10),
		&[StepPhaseTiming {
			label: "prepare registry request".to_string(),
			duration: Duration::from_millis(8),
		}],
	);
	reporter.command_finished(Duration::from_millis(12));

	let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
	assert!(!output.contains('\r'));
	assert!(!output.contains('\u{1b}'));
	assert!(output.lines().any(|line| line == "▶ publish packages"));
	assert!(
		output
			.lines()
			.any(|line| line.contains("Publishing 1 package (Release dry-run)"))
	);
	assert!(
		output
			.lines()
			.any(|line| line.contains("would publish 1.2.3"))
	);
}

#[test]
fn progress_and_diagnostics_use_the_same_writer() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Auto);
	reporter.phase_started("Loading workspace configuration");
	let diagnostic = CliDiagnostic::from_error(
		&monochange_core::MonochangeError::Config("missing package".to_string()),
		Some("monochange check"),
	);
	reporter.write_diagnostic(diagnostic);

	let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
	assert_eq!(
		output,
		"▶ Loading workspace configuration\nerror[config.invalid]: missing package\n  command: monochange check\n  help:    Check `monochange.toml` and the command arguments, then rerun the command.\n",
	);
}

#[test]
fn json_view_sequences_lint_and_publish_events_from_the_same_reporter() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Json);
	LintProgressReporter::planning_started(&reporter, &["cargo"]);
	PublishProgressReporter::report(
		&reporter,
		PublishProgressEvent::RunStarted {
			mode: PackagePublishRunMode::Release,
			dry_run: true,
			total: 1,
			ecosystems: vec![Ecosystem::Cargo],
		},
	);

	let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
	let events = output
		.lines()
		.map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
		.collect::<Vec<_>>();
	assert_eq!(events.len(), 2);
	let mut events = events.iter();
	let lint = events.next().expect("lint event");
	let publish = events.next().expect("publish event");
	assert_eq!(lint.get("sequence"), Some(&serde_json::json!(0)));
	assert_eq!(
		lint.get("event"),
		Some(&serde_json::json!("lint_planning_started"))
	);
	assert_eq!(publish.get("sequence"), Some(&serde_json::json!(1)));
	assert_eq!(
		publish.get("event"),
		Some(&serde_json::json!("publish_run_started"))
	);
}

#[test]
fn publish_event_rendering_keeps_the_package_and_outcome_prominent() {
	assert_eq!(
		render_publish_event(
			&PublishProgressEvent::PackageStarted(publish_package()),
			&UNICODE_SYMBOLS,
			false,
			false,
		),
		"▶ 🦀 cargo monochange publishing 1.2.3 to crates.io"
	);
	// The spinner supplies the activity frame, so the message must not repeat it.
	assert_eq!(
		render_publish_event(
			&PublishProgressEvent::PackageStarted(publish_package()),
			&UNICODE_SYMBOLS,
			false,
			true,
		),
		"🦀 cargo monochange publishing 1.2.3 to crates.io"
	);
	assert_eq!(
		render_publish_event(
			&PublishProgressEvent::PackageFailed {
				package: publish_package(),
				message: "registry rejected package".to_string(),
			},
			&ASCII_SYMBOLS,
			false,
			false,
		),
		"x cargo monochange failed: registry rejected package"
	);
}

#[test]
fn json_progress_covers_phases_warnings_command_output_and_lint_lifecycle() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Json);
	let step = named_command_step("check workspace");
	reporter.phase_started("Loading configuration");
	reporter.phase_finished("Loaded configuration", Duration::from_millis(4));
	reporter.phase_failed(
		"Validate configuration",
		Duration::from_millis(5),
		"invalid",
	);
	reporter.warning("deprecated option");
	reporter.log_command_output(0, &step, CommandStream::Stdout, "result");
	reporter.log_command_output(0, &step, CommandStream::Stderr, "warning");

	LintProgressReporter::planning_started(&reporter, &["cargo", "changeset"]);
	LintProgressReporter::planning_finished(&reporter, 3, 4);
	LintProgressReporter::suite_started(&reporter, "cargo", 2, 3);
	LintProgressReporter::suite_finished(&reporter, "cargo", 1, 1);
	LintProgressReporter::fix_started(&reporter, 2);
	LintProgressReporter::fix_applied(
		&reporter,
		std::path::Path::new("Cargo.toml"),
		"sorted dependencies",
	);
	LintProgressReporter::fix_finished(&reporter, 1);
	LintProgressReporter::summary(&reporter, 1, 2, 1, false);

	let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
	let events = output
		.lines()
		.map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
		.collect::<Vec<_>>();
	let names = events
		.iter()
		.filter_map(|event| event.get("event").and_then(serde_json::Value::as_str))
		.collect::<Vec<_>>();
	assert_eq!(
		names,
		[
			"phase_started",
			"phase_finished",
			"phase_failed",
			"warning",
			"command_output",
			"command_output",
			"lint_planning_started",
			"lint_planning_finished",
			"lint_suite_started",
			"lint_suite_finished",
			"lint_fix_started",
			"lint_fix_applied",
			"lint_fix_finished",
			"lint_summary",
		]
	);
}

#[test]
fn human_progress_covers_lint_counts_fixes_and_summary_wording() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Auto);
	LintProgressReporter::planning_started(&reporter, &[]);
	LintProgressReporter::planning_started(&reporter, &["cargo"]);
	LintProgressReporter::planning_started(&reporter, &["cargo", "changeset"]);
	LintProgressReporter::planning_finished(&reporter, 2, 3);
	LintProgressReporter::suite_started(&reporter, "cargo", 1, 1);
	LintProgressReporter::suite_started(&reporter, "changeset", 2, 3);
	LintProgressReporter::suite_finished(&reporter, "cargo", 1, 0);
	LintProgressReporter::suite_finished(&reporter, "changeset", 2, 1);
	LintProgressReporter::fix_started(&reporter, 1);
	LintProgressReporter::fix_started(&reporter, 2);
	LintProgressReporter::fix_applied(
		&reporter,
		std::path::Path::new("Cargo.toml"),
		"sorted dependencies",
	);
	LintProgressReporter::fix_finished(&reporter, 1);
	LintProgressReporter::fix_finished(&reporter, 2);
	LintProgressReporter::summary(&reporter, 0, 0, 0, false);
	LintProgressReporter::summary(&reporter, 1, 1, 1, false);
	LintProgressReporter::summary(&reporter, 2, 2, 2, true);

	insta::assert_snapshot!(recorded_text(&bytes));
}

#[test]
fn publish_rendering_and_json_cover_every_event_variant() {
	let package = publish_package();
	let events = [
		PublishProgressEvent::RunStarted {
			mode: PackagePublishRunMode::Release,
			dry_run: false,
			total: 2,
			ecosystems: vec![Ecosystem::Cargo, Ecosystem::Npm],
		},
		PublishProgressEvent::RegistryCheckStarted(package.clone()),
		PublishProgressEvent::PackageStarted(package.clone()),
		PublishProgressEvent::PackageSkipped {
			package: package.clone(),
			message: "already exists".to_string(),
		},
		PublishProgressEvent::PackagePlanned(package.clone()),
		PublishProgressEvent::PackagePublished(package.clone()),
		PublishProgressEvent::PackageFailed {
			package,
			message: "registry rejected package".to_string(),
		},
		PublishProgressEvent::RunFinished {
			mode: PackagePublishRunMode::Release,
			total: 2,
			published: 1,
			staged: 0,
			skipped: 0,
			failed: 1,
		},
	];

	let rendered = events
		.iter()
		.map(|event| render_publish_event(event, &UNICODE_SYMBOLS, false, false))
		.collect::<Vec<_>>();
	assert!(rendered[0].contains("across 🦀 cargo, 📦 npm"));
	assert_eq!(
		rendered[1],
		"▶ 🦀 cargo monochange checking 1.2.3 on crates.io"
	);
	assert!(rendered[3].contains("already exists"));
	assert!(rendered[5].contains("published 1.2.3"));
	assert_eq!(rendered[7], "✖ Publish complete: 1 published, 1 failed");
	assert_eq!(
		render_publish_event(
			&PublishProgressEvent::RunFinished {
				mode: PackagePublishRunMode::Release,
				total: 5,
				published: 1,
				staged: 0,
				skipped: 0,
				failed: 0,
			},
			&ASCII_SYMBOLS,
			false,
			false,
		),
		"+ Publish complete: 1 published, 4 not attempted"
	);
	// Nothing published is a neutral outcome, not a success or a failure.
	assert_eq!(
		render_publish_event(
			&PublishProgressEvent::RunFinished {
				mode: PackagePublishRunMode::Release,
				total: 3,
				published: 0,
				staged: 0,
				skipped: 3,
				failed: 0,
			},
			&UNICODE_SYMBOLS,
			false,
			false,
		),
		"· Publish complete: 0 published, 3 skipped"
	);
	assert_eq!(
		render_publish_event(
			&PublishProgressEvent::RunStarted {
				mode: PackagePublishRunMode::Placeholder,
				dry_run: false,
				total: 1,
				ecosystems: Vec::new(),
			},
			&UNICODE_SYMBOLS,
			false,
			false,
		),
		"▶ Publishing 1 package (Placeholder)"
	);

	let empty_run = render_publish_event(
		&PublishProgressEvent::RunStarted {
			mode: PackagePublishRunMode::Placeholder,
			dry_run: true,
			total: 0,
			ecosystems: vec![Ecosystem::Npm],
		},
		&ASCII_SYMBOLS,
		false,
		false,
	);
	assert_eq!(
		empty_run,
		"> Publishing 0 packages (Placeholder dry-run) across npm"
	);

	let (reporter, bytes) = recorded_reporter(ProgressFormat::Json);
	for event in events {
		PublishProgressReporter::report(&reporter, event);
	}
	let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
	let names = output
		.lines()
		.map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
		.filter_map(|value| value["event"].as_str().map(ToString::to_string))
		.collect::<Vec<_>>();
	assert_eq!(
		names,
		[
			"publish_run_started",
			"publish_registry_check_started",
			"publish_package_started",
			"publish_package_skipped",
			"publish_package_planned",
			"publish_package_published",
			"publish_package_failed",
			"publish_run_finished",
		]
	);
}

#[test]
fn progress_disabled_quiet_colored_and_animated_edges_are_safe() {
	let step = named_command_step("publish packages");
	let disabled = progress_reporter(false, false);
	disabled.phase_failed("phase", Duration::ZERO, "failed");
	disabled.log_command_output(0, &step, CommandStream::Stdout, "ignored");
	disabled.log_command_output(0, &step, CommandStream::Stdout, "");
	LintProgressReporter::suite_started(&disabled, "cargo", 1, 1);
	LintProgressReporter::suite_finished(&disabled, "cargo", 0, 0);
	LintProgressReporter::fix_started(&disabled, 1);
	LintProgressReporter::fix_applied(&disabled, std::path::Path::new("file"), "fix");
	LintProgressReporter::fix_finished(&disabled, 1);
	LintProgressReporter::summary(&disabled, 1, 0, 0, false);
	PublishProgressReporter::report(
		&disabled,
		PublishProgressEvent::PackageStarted(publish_package()),
	);
	disabled.warning("ignored");

	let (mut colored, bytes) = recorded_reporter(ProgressFormat::Auto);
	colored.color = true;
	colored.phase_failed("Build", Duration::from_millis(1), "failed");
	colored.log_command_output(0, &step, CommandStream::Stdout, "\u{1b}[32mkept\u{1b}[0m");
	assert!(
		String::from_utf8(bytes.lock().unwrap().clone())
			.unwrap()
			.contains("\u{1b}[32mkept")
	);

	let mut animated = progress_reporter(true, false);
	animated.animate = true;
	LintProgressReporter::suite_started(&animated, "cargo", 1, 1);
	LintProgressReporter::suite_finished(&animated, "cargo", 0, 0);
	LintProgressReporter::fix_started(&animated, 1);
	LintProgressReporter::fix_finished(&animated, 1);
	PublishProgressReporter::report(
		&animated,
		PublishProgressEvent::RegistryCheckStarted(publish_package()),
	);
	PublishProgressReporter::report(
		&animated,
		PublishProgressEvent::PackagePlanned(publish_package()),
	);
}

#[test]
fn terminal_control_stripping_ignores_incomplete_or_unknown_escape_sequences() {
	assert_eq!(strip_terminal_controls("a\u{1b}xb"), "ab");
	assert_eq!(strip_terminal_controls("a\u{1b}"), "a");
	assert_eq!(strip_terminal_controls("a\u{1b}]title\u{1b}\\b"), "ab");
}

#[test]
fn verbose_progress_lists_every_phase_of_fast_steps() {
	let step = named_command_step("plan release");
	let (reporter, bytes) = recorded_reporter_with(
		1,
		TerminalCapabilities {
			verbose: true,
			..capabilities(false, false)
		},
		ProgressFormat::Unicode,
	);
	let phases = (1..=7)
		.map(|index| {
			StepPhaseTiming {
				label: format!("phase {index}"),
				duration: Duration::from_millis(index),
			}
		})
		.collect::<Vec<_>>();
	reporter.step_finished(0, &step, Duration::from_millis(40), &phases);

	let text = recorded_text(&bytes);
	assert_eq!(text.matches("phase ").count(), 7, "{text}");
	assert!(text.contains("phase 1"), "{text}");
}

#[test]
fn json_progress_reports_failures_as_diagnostic_events() {
	let (reporter, bytes) = recorded_reporter(ProgressFormat::Json);
	reporter.write_diagnostic(CliDiagnostic::from_error(
		&monochange_core::MonochangeError::Config("bad input".to_string()),
		Some("monochange run release"),
	));

	let text = recorded_text(&bytes);
	let event: serde_json::Value = serde_json::from_str(text.trim())
		.unwrap_or_else(|error| panic!("diagnostic event: {error}\n{text}"));
	assert_eq!(event["event"], "diagnostic");
	assert_eq!(event["code"], "config.invalid");
	assert_eq!(event["summary"], "bad input");
	assert_eq!(event["context"]["command"], "monochange run release");
	assert_eq!(event["exit_code"], 1);
	assert!(!text.contains("error["), "{text}");
}

#[test]
fn long_running_spinners_show_the_elapsed_time() {
	let (reporter, bytes) =
		recorded_reporter_with(1, capabilities(true, false), ProgressFormat::Unicode);
	let started_at = Instant::now()
		.checked_sub(Duration::from_secs(5))
		.unwrap_or_else(|| panic!("an instant five seconds ago"));
	reporter.start_spinner("compile workspace".to_string(), Some(started_at));
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(20));
	reporter.stop_spinner();

	let output = recorded_text(&bytes);
	assert!(output.contains("compile workspace 5s"), "{output}");
}
