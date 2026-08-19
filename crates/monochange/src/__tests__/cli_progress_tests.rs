#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::ShellConfig;

use super::*;

fn progress_reporter(enabled: bool, color: bool) -> CliProgressReporter {
	CliProgressReporter {
		enabled,
		color,
		animate: false,
		command_name: "release".to_string(),
		dry_run: false,
		total_steps: 3,
		active_spinner: None,
		command_started: false,
		render_mode: ProgressRenderMode::Human,
		symbols: UNICODE_SYMBOLS,
		event_sequence: 0,
	}
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
	assert_eq!(format_duration(Duration::from_secs(61)), "61.0s");
	assert_eq!(format_duration(Duration::from_millis(1500)), "1.50s");
	assert_eq!(format_duration(Duration::from_micros(12)), "12µs");
}

#[test]
fn progress_format_parsing_and_renderer_selection_cover_all_variants() {
	let command = command_with_step(named_command_step("announce release"));
	assert_eq!(ProgressFormat::parse("auto"), Some(ProgressFormat::Auto));
	assert_eq!(
		ProgressFormat::parse("unicode"),
		Some(ProgressFormat::Unicode)
	);
	assert_eq!(ProgressFormat::parse("ascii"), Some(ProgressFormat::Ascii));
	assert_eq!(ProgressFormat::parse("json"), Some(ProgressFormat::Json));
	assert_eq!(ProgressFormat::parse("wat"), None);

	let unicode = CliProgressReporter::new(&command, false, false, ProgressFormat::Unicode);
	assert!(unicode.enabled);
	assert_eq!(unicode.render_mode, ProgressRenderMode::Human);
	assert_eq!(
		unicode.symbols.command_success,
		UNICODE_SYMBOLS.command_success
	);

	let ascii = CliProgressReporter::new(&command, false, false, ProgressFormat::Ascii);
	assert!(ascii.enabled);
	assert_eq!(ascii.render_mode, ProgressRenderMode::Human);
	assert_eq!(ascii.symbols.command_success, ASCII_SYMBOLS.command_success);

	let auto = CliProgressReporter::new(&command, false, false, ProgressFormat::Auto);
	assert!(auto.enabled);

	let quiet = CliProgressReporter::new(&command, false, true, ProgressFormat::Auto);
	assert!(!quiet.enabled);

	let json = CliProgressReporter::new(&command, false, false, ProgressFormat::Json);
	assert!(json.enabled);
	assert_eq!(json.render_mode, ProgressRenderMode::Json);
	assert_eq!(json.symbols.command_success, ASCII_SYMBOLS.command_success);
}

#[test]
fn progress_reporter_renders_skips_failures_and_stderr_output_when_enabled() {
	let mut reporter = progress_reporter(true, false);
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
	let mut disabled = progress_reporter(false, false);
	disabled.step_status(0, &step, "locating release record");

	let mut human = progress_reporter(true, false);
	human.step_status(0, &step, "planning retarget");

	let mut json = progress_reporter(true, false);
	json.render_mode = ProgressRenderMode::Json;
	json.step_status(0, &step, "applying git ref and provider updates");
	assert_eq!(json.event_sequence, 1);

	let mut animated = progress_reporter(true, true);
	animated.animate = true;
	animated.step_status(0, &step, "syncing provider metadata");
	assert!(animated.active_spinner.is_some());
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
	// Simulate another writer (for example `print_line` or publish progress)
	// clearing the spinner line: the next tick must restore the full line.
	mark_spinner_line_cleared();
	thread::sleep(SPINNER_TICK + Duration::from_millis(20));
	reporter.step_finished(0, &step, Duration::from_millis(12), &[]);
	reporter.command_finished(Duration::from_millis(25));
}

#[test]
fn log_command_output_appends_ansi_reset_after_raw_lines() {
	let _step = named_command_step("prepare");
	// Simulate a subprocess emitting ANSI yellow/brown without a trailing reset
	let raw_line = "\x1b[33mwarning: something happened";

	// The format string in log_command_output appends \x1b[0m to each line
	let formatted = format!(
		"  {} {} {}\u{1b}[0m",
		"│", // simplified pipe
		"prepare [stderr]",
		raw_line,
	);

	// Verify the ANSI reset is present at the end of the line
	assert!(
		formatted.ends_with("\x1b[0m"),
		"raw subprocess output must end with ANSI reset, got: {formatted:?}"
	);

	// Verify the reset appears after the raw content, not before
	let reset_pos = formatted.rfind("\x1b[0m").unwrap();
	let raw_pos = formatted.find(raw_line).unwrap();
	assert!(
		reset_pos > raw_pos,
		"ANSI reset must appear after raw subprocess output"
	);
}
