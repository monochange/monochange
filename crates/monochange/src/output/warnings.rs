//! Render `tracing` warnings for people instead of for maintainers.
//!
//! Warnings such as a GitHub API fallback or a publish retry used to be
//! visible only with `--log-level`, buried in span-prefixed trace records.
//! This layer shows each `WARN` event as one readable `warning:` line with its
//! fields underneath, through the same stderr channel as progress output.

use std::collections::BTreeSet;
use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::Mutex;

use tracing::Event;
use tracing::Level;
use tracing::Subscriber;
use tracing::field::Field;
use tracing::field::Visit;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::SubscriberExt;

use crate::output::terminal::SharedStderr;

/// How warnings are written to stderr.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WarningMode {
	/// `warning: message` followed by indented fields.
	Human,
	/// A GitHub Actions `::warning` workflow command, which GitHub renders as
	/// a highlighted log line and adds to the run's annotations.
	GitHub,
	/// A newline-delimited `warning` event for `--progress-format json`.
	Json,
}

/// Writes rendered warnings to the shared stderr channel.
///
/// Configuration can be loaded more than once in a run, so the sink shows
/// each distinct warning once instead of repeating it.
#[derive(Clone)]
pub(crate) struct WarningSink {
	stderr: SharedStderr,
	mode: WarningMode,
	color: bool,
	seen: Arc<Mutex<BTreeSet<String>>>,
}

impl WarningSink {
	pub(crate) fn new(stderr: SharedStderr, mode: WarningMode, color: bool) -> Self {
		Self {
			stderr,
			mode,
			color,
			seen: Arc::new(Mutex::new(BTreeSet::new())),
		}
	}

	pub(crate) fn emit(&self, message: &str, fields: &[(String, String)]) {
		let key = format!("{message}{fields:?}");
		if !self.seen.lock().is_ok_and(|mut seen| seen.insert(key)) {
			return;
		}
		match self.mode {
			WarningMode::Human => {
				self.stderr
					.write_line(&render_human_warning(message, fields, self.color));
			}
			WarningMode::GitHub => {
				let mut annotation = message.to_string();
				for (name, value) in fields {
					let _ = write!(annotation, "\n{}: {value}", field_label(name));
				}
				// A folded log group would hide the warning line.
				self.stderr.close_group();
				self.stderr.write_line(&format!(
					"::warning title=monochange::{}",
					escape_workflow_data(&annotation)
				));
			}
			WarningMode::Json => {
				let fields = fields
					.iter()
					.map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
					.collect::<serde_json::Map<_, _>>();
				let event = serde_json::json!({
					"sequence": self.stderr.next_sequence(),
					"event": "warning",
					"message": message,
					"fields": fields,
				});
				self.stderr.write_line(&event.to_string());
			}
		}
	}
}

/// A `tracing` layer that forwards `WARN` events to a [`WarningSink`].
///
/// `ERROR` events are left to the command result and final diagnostic, which
/// already report every failure that monochange returns.
pub(crate) struct WarningLayer {
	sink: WarningSink,
}

impl WarningLayer {
	pub(crate) fn new(sink: WarningSink) -> Self {
		Self { sink }
	}
}

impl<S: Subscriber> Layer<S> for WarningLayer {
	fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
		if *event.metadata().level() != Level::WARN {
			return;
		}
		let mut visitor = WarningVisitor::default();
		event.record(&mut visitor);
		self.sink.emit(&visitor.message, &visitor.fields);
	}
}

/// A subscriber that renders only `WARN` events through `sink`.
///
/// The level filter disables every more verbose callsite, so the spans and
/// `debug!` events used for maintainer tracing cost nothing by default.
pub(crate) fn warning_subscriber(sink: WarningSink) -> impl Subscriber + Send + Sync {
	tracing_subscriber::registry().with(WarningLayer::new(sink).with_filter(LevelFilter::WARN))
}

#[derive(Default)]
struct WarningVisitor {
	message: String,
	fields: Vec<(String, String)>,
}

impl Visit for WarningVisitor {
	fn record_str(&mut self, field: &Field, value: &str) {
		self.record(field, value.to_string());
	}

	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
		self.record(field, format!("{value:?}"));
	}
}

impl WarningVisitor {
	fn record(&mut self, field: &Field, value: String) {
		if field.name() == "message" {
			self.message = value;
		} else {
			self.fields.push((field.name().to_string(), value));
		}
	}
}

fn render_human_warning(message: &str, fields: &[(String, String)], color: bool) -> String {
	let mut output = format!("{} {message}", paint("warning:", "33;1", color));
	let width = fields
		.iter()
		.map(|(name, _)| name.chars().count())
		.max()
		.unwrap_or_default();
	for (name, value) in fields {
		let label = format!("{}:", field_label(name));
		let _ = write!(
			output,
			"\n  {}{} {value}",
			paint(&label, "2", color),
			" ".repeat(width.saturating_sub(name.chars().count())),
		);
	}
	output
}

/// `commit_sha` reads as `commit sha` for people; JSON keeps the field name.
fn field_label(name: &str) -> String {
	name.replace('_', " ")
}

fn paint(text: &str, code: &str, color: bool) -> String {
	if color {
		format!("\u{1b}[{code}m{text}\u{1b}[0m")
	} else {
		text.to_string()
	}
}

/// Escape a GitHub Actions workflow command message.
pub(crate) fn escape_workflow_data(text: &str) -> String {
	text.replace('%', "%25")
		.replace('\r', "%0D")
		.replace('\n', "%0A")
}

/// Escape a GitHub Actions workflow command property such as `title`.
pub(crate) fn escape_workflow_property(text: &str) -> String {
	escape_workflow_data(text)
		.replace(':', "%3A")
		.replace(',', "%2C")
}

#[cfg(test)]
#[path = "../__tests__/output_warnings_tests.rs"]
mod tests;
