//! Building blocks for human command results on stdout.
//!
//! Every text result follows the same shape: a headline that answers the
//! command's question, then titled sections with aligned tables or short
//! lists. Long lists are truncated with a pointer to `--format json`, which
//! always carries the complete data, and `--verbose` shows them in full.
//! Colour is applied only when stdout is an interactive terminal, so captured
//! output and CI logs stay plain.

use std::future::Future;
use std::io::IsTerminal;

/// How a piece of result text is emphasized in a terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tone {
	Plain,
	Heading,
	Success,
	Warning,
	Error,
	Muted,
	Accent,
	Value,
}

tokio::task_local! {
	/// `--verbose` for the command running on this task.
	static VERBOSE: bool;
}

/// Run `future` with `--verbose` result rendering turned on or off.
///
/// A task-local keeps concurrent invocations (such as parallel tests) from
/// seeing each other's setting.
pub(crate) async fn with_verbosity<F: Future>(verbose: bool, future: F) -> F::Output {
	VERBOSE.scope(verbose, future).await
}

/// Whether the running command asked for `--verbose` output.
pub(crate) fn verbose_output() -> bool {
	VERBOSE.try_with(|verbose| *verbose).unwrap_or(false)
}

/// How result text on stdout is presented: ANSI styling, and whether long
/// lists and logs are shown in full (`--verbose`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TextTheme {
	color: bool,
	verbose: bool,
}

impl TextTheme {
	/// Colour only for an interactive stdout that has not opted out.
	pub(crate) fn for_stdout() -> Self {
		Self {
			color: !cfg!(test)
				&& std::io::stdout().is_terminal()
				&& std::env::var_os("NO_COLOR").is_none()
				&& std::env::var("TERM").is_ok_and(|term| term != "dumb"),
			verbose: verbose_output(),
		}
	}

	pub(crate) fn paint(self, text: &str, tone: Tone) -> String {
		let code = match tone {
			Tone::Plain => return text.to_string(),
			Tone::Heading => "1",
			Tone::Success => "32;1",
			Tone::Warning => "33;1",
			Tone::Error => "31;1",
			Tone::Muted => "2",
			Tone::Accent => "36",
			Tone::Value => "32",
		};
		if self.color {
			format!("\u{1b}[{code}m{text}\u{1b}[0m")
		} else {
			text.to_string()
		}
	}
}

/// The outcome a headline reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
	Success,
	Warning,
	Failure,
	Neutral,
}

impl Outcome {
	fn symbol(self) -> (&'static str, Tone) {
		match self {
			Self::Success => ("✔", Tone::Success),
			Self::Warning => ("▲", Tone::Warning),
			Self::Failure => ("✖", Tone::Error),
			Self::Neutral => ("•", Tone::Accent),
		}
	}
}

/// One cell of a [`TextReport::table`] row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableCell {
	text: String,
	tone: Tone,
}

impl TableCell {
	pub(crate) fn new(text: impl Into<String>, tone: Tone) -> Self {
		Self {
			text: text.into(),
			tone,
		}
	}

	pub(crate) fn plain(text: impl Into<String>) -> Self {
		Self::new(text, Tone::Plain)
	}
}

/// A human command result assembled from a headline and sections.
pub(crate) struct TextReport {
	theme: TextTheme,
	lines: Vec<String>,
	/// Indexes of lines from [`TextReport::raw_block`], which keep their
	/// trailing whitespace (a diff context line is a single space).
	verbatim: std::collections::BTreeSet<usize>,
}

impl TextReport {
	pub(crate) fn new(theme: TextTheme) -> Self {
		Self {
			theme,
			lines: Vec::new(),
			verbatim: std::collections::BTreeSet::new(),
		}
	}

	/// The first line of the result, such as `✔ Prepared release · dry-run`.
	pub(crate) fn headline(&mut self, outcome: Outcome, text: &str, details: &[String]) {
		let (symbol, tone) = outcome.symbol();
		let mut line = format!(
			"{} {}",
			self.theme.paint(symbol, tone),
			self.theme.paint(text, Tone::Heading)
		);
		if !details.is_empty() {
			line.push_str(
				&self
					.theme
					.paint(&format!(" · {}", details.join(" · ")), Tone::Muted),
			);
		}
		self.push_separated(line);
	}

	/// Start a titled section, such as `Changed files (12)`.
	pub(crate) fn section(&mut self, title: &str, count: Option<usize>) {
		let mut line = self.theme.paint(title, Tone::Heading);
		if let Some(count) = count {
			line.push_str(&self.theme.paint(&format!(" ({count})"), Tone::Muted));
		}
		self.push_separated(line);
	}

	/// Rows of cells aligned into columns and indented under a section.
	pub(crate) fn table(&mut self, rows: &[Vec<TableCell>]) {
		self.nested_table(rows, 2);
	}

	/// A [`TextReport::table`] indented `indent` spaces, for tables that
	/// belong to an item inside a section.
	pub(crate) fn nested_table(&mut self, rows: &[Vec<TableCell>], indent: usize) {
		let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
		let widths = (0..columns)
			.map(|column| {
				rows.iter()
					.filter_map(|row| row.get(column))
					.map(|cell| display_width(&cell.text))
					.max()
					.unwrap_or_default()
			})
			.collect::<Vec<_>>();
		for row in rows {
			let mut line = " ".repeat(indent);
			for (column, cell) in row.iter().enumerate() {
				let is_last = column + 1 == row.len();
				line.push_str(&self.theme.paint(&cell.text, cell.tone));
				if !is_last {
					let width = widths.get(column).copied().unwrap_or_default();
					line.push_str(&" ".repeat(width - display_width(&cell.text) + 2));
				}
			}
			self.lines.push(line.trim_end().to_string());
		}
	}

	/// Items indented under a section, truncated after `limit` entries
	/// unless the theme is verbose.
	pub(crate) fn list<I>(&mut self, items: I, limit: usize)
	where
		I: IntoIterator<Item = String>,
	{
		let limit = if self.theme.verbose {
			usize::MAX
		} else {
			limit
		};
		let items = items.into_iter().collect::<Vec<_>>();
		for item in items.iter().take(limit) {
			let mut item_lines = item.lines();
			self.lines
				.push(format!("  {}", item_lines.next().unwrap_or_default()));
			// Continuation lines (such as a source snippet inside a warning)
			// stay aligned under the item instead of hitting the margin.
			self.lines
				.extend(item_lines.map(|line| format!("    {line}")));
		}
		if items.len() > limit {
			self.lines.push(self.theme.paint(
				&format!(
					"  … and {} more (use --format json for the full list)",
					items.len() - limit
				),
				Tone::Muted,
			));
		}
	}

	/// Labels and values aligned in two columns, such as `Manifest  path`.
	pub(crate) fn fields(&mut self, rows: &[(&str, String)]) {
		let width = rows
			.iter()
			.map(|(label, _)| display_width(label))
			.max()
			.unwrap_or_default();
		for (index, (label, value)) in rows.iter().enumerate() {
			let line = format!(
				"{}{}  {value}",
				self.theme.paint(label, Tone::Muted),
				" ".repeat(width - display_width(label))
			);
			if index == 0 {
				self.push_separated(line);
			} else {
				self.lines.push(line);
			}
		}
	}

	/// A line indented under the current section.
	pub(crate) fn indented(&mut self, text: &str, tone: Tone) {
		self.lines
			.push(format!("  {}", self.theme.paint(text, tone)));
	}

	/// A paragraph separated from the previous block by a blank line.
	pub(crate) fn paragraph(&mut self, text: &str, tone: Tone) {
		let line = self.theme.paint(text, tone);
		self.push_separated(line);
	}

	/// Verbatim lines, such as a rendered diff, separated by a blank line.
	pub(crate) fn raw_block(&mut self, text: &str) {
		if !self.lines.is_empty() {
			self.lines.push(String::new());
		}
		for line in text.lines() {
			self.verbatim.insert(self.lines.len());
			self.lines.push(line.to_string());
		}
	}

	pub(crate) fn is_empty(&self) -> bool {
		self.lines.is_empty()
	}

	pub(crate) fn is_verbose(&self) -> bool {
		self.theme.verbose
	}

	pub(crate) fn render(self) -> String {
		let mut lines = self
			.lines
			.iter()
			.enumerate()
			.map(|(index, line)| {
				if self.verbatim.contains(&index) {
					line.as_str()
				} else {
					line.trim_end()
				}
			})
			.collect::<Vec<_>>();
		while lines.last().is_some_and(|line| line.trim().is_empty()) {
			lines.pop();
		}
		lines.join("\n")
	}

	fn push_separated(&mut self, line: String) {
		if !self.lines.is_empty() {
			self.lines.push(String::new());
		}
		self.lines.push(line);
	}
}

/// The first line of `text`, shortened to `limit` characters with an ellipsis.
pub(crate) fn first_line_preview(text: &str, limit: usize) -> String {
	let line = text
		.lines()
		.map(str::trim)
		.find(|line| !line.is_empty())
		.unwrap_or_default();
	let more_lines = text.lines().filter(|line| !line.trim().is_empty()).count() > 1;
	if display_width(line) <= limit {
		return if more_lines {
			format!("{line} …")
		} else {
			line.to_string()
		};
	}
	let shortened = line.chars().take(limit).collect::<String>();
	format!("{}…", shortened.trim_end())
}

/// Collapse a multi-line command log to its last line, noting how much was
/// omitted, so a result summary never replays a whole build log.
pub(crate) fn summarize_log(text: &str) -> String {
	let lines = text
		.lines()
		.map(str::trim_end)
		.filter(|line| !line.trim().is_empty())
		.collect::<Vec<_>>();
	match lines.as_slice() {
		[] => String::new(),
		[line] => (*line).to_string(),
		[.., last] => format!("{} (+{} earlier lines)", last.trim(), lines.len() - 1),
	}
}

pub(crate) fn display_width(text: &str) -> usize {
	text.chars().count()
}

pub(crate) fn plural(count: usize, singular: &str, plural: &str) -> String {
	format!("{count} {}", if count == 1 { singular } else { plural })
}

#[cfg(test)]
#[path = "../__tests__/output_text_tests.rs"]
mod tests;
