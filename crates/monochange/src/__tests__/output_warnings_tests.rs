use std::io;
use std::sync::Arc;
use std::sync::Mutex;

use super::*;

#[derive(Clone)]
struct RecordedWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for RecordedWriter {
	fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
		self.0.lock().unwrap().extend_from_slice(bytes);
		Ok(bytes.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

fn recorded_sink(mode: WarningMode, color: bool) -> (WarningSink, Arc<Mutex<Vec<u8>>>) {
	let bytes = Arc::new(Mutex::new(Vec::new()));
	let stderr = SharedStderr::with_writer(RecordedWriter(Arc::clone(&bytes)));
	(WarningSink::new(stderr, mode, color), bytes)
}

fn emit_sample_events(sink: WarningSink) {
	tracing::subscriber::with_default(warning_subscriber(sink), || {
		tracing::info!("hidden info");
		tracing::error!("errors are reported by the command result");
		tracing::warn!(
			reason = %"GitHub API returned 422",
			commit_sha = "abc1234",
			"could not create a verified release commit"
		);
		tracing::warn!(
			reason = %"GitHub API returned 422",
			commit_sha = "abc1234",
			"could not create a verified release commit"
		);
		tracing::warn!(attempt = 2, "publish command attempt failed");
	});
}

fn recorded_text(bytes: &Arc<Mutex<Vec<u8>>>) -> String {
	String::from_utf8(bytes.lock().unwrap().clone()).unwrap()
}

#[test]
fn human_warnings_show_the_message_and_aligned_fields_once() {
	let (sink, bytes) = recorded_sink(WarningMode::Human, false);
	emit_sample_events(sink);

	insta::assert_snapshot!(recorded_text(&bytes));
}

#[test]
fn colored_warnings_paint_the_label_and_field_names() {
	let (sink, bytes) = recorded_sink(WarningMode::Human, true);
	sink.emit("slow", &[("attempt".to_string(), "2".to_string())]);

	assert_eq!(
		recorded_text(&bytes),
		"\u{1b}[33;1mwarning:\u{1b}[0m slow\n  \u{1b}[2mattempt:\u{1b}[0m 2\n"
	);
}

#[test]
fn github_warnings_become_escaped_workflow_annotations() {
	let (sink, bytes) = recorded_sink(WarningMode::GitHub, false);
	emit_sample_events(sink);

	insta::assert_snapshot!(recorded_text(&bytes));
}

#[test]
fn json_warnings_are_sequenced_events_with_fields() {
	let (sink, bytes) = recorded_sink(WarningMode::Json, false);
	emit_sample_events(sink);

	let events = recorded_text(&bytes)
		.lines()
		.map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
		.collect::<Vec<_>>();
	insta::assert_json_snapshot!(events);
}

#[test]
fn workflow_command_escaping_protects_messages_and_properties() {
	assert_eq!(escape_workflow_data("50%\r\nnext"), "50%25%0D%0Anext");
	assert_eq!(
		escape_workflow_property("monochange: run, release"),
		"monochange%3A run%2C release"
	);
}
