use super::*;

fn plain() -> TextTheme {
	TextTheme {
		color: false,
		verbose: false,
		ascii: false,
	}
}

fn colored() -> TextTheme {
	TextTheme {
		color: true,
		verbose: false,
		ascii: false,
	}
}

fn ascii() -> TextTheme {
	TextTheme {
		color: false,
		verbose: false,
		ascii: true,
	}
}

#[test]
fn reports_lead_with_a_headline_and_separate_sections() {
	let mut report = TextReport::new(plain());
	assert!(report.is_empty());
	report.headline(
		Outcome::Success,
		"Prepared release",
		&["2 packages".to_string(), "3 files changed".to_string()],
	);
	report.section("Releases", None);
	report.table(&[
		vec![
			TableCell::new("sdk", Tone::Heading),
			TableCell::new("v1.1.0", Tone::Value),
			TableCell::new("group · tag", Tone::Muted),
		],
		vec![
			TableCell::plain("classification"),
			TableCell::plain("classification/v0.3.1"),
		],
	]);
	report.section("Changed files", Some(3));
	report.list(["a.toml", "b.toml", "c.toml"].map(ToString::to_string), 2);
	report.fields(&[
		("Manifest", "release.json".to_string()),
		("Log", "notes".to_string()),
	]);

	report.paragraph("Run it again.", Tone::Plain);
	report.raw_block("--- a\n+++ b");
	report.indented("detail", Tone::Muted);
	assert!(!report.is_empty());

	insta::assert_snapshot!(report.render());
}

#[test]
fn every_outcome_has_its_own_symbol() {
	let mut report = TextReport::new(plain());
	report.headline(Outcome::Success, "ok", &[]);
	report.headline(Outcome::Warning, "careful", &[]);
	report.headline(Outcome::Failure, "broken", &[]);
	report.headline(Outcome::Neutral, "note", &[]);

	assert_eq!(report.render(), "✔ ok\n\n▲ careful\n\n✖ broken\n\n• note");
}

#[test]
fn verbose_themes_show_every_list_item() {
	let verbose = TextTheme {
		color: false,
		verbose: true,
		ascii: false,
	};
	let mut report = TextReport::new(verbose);
	assert!(report.is_verbose());
	report.list(["a", "b", "c"].map(ToString::to_string), 1);

	assert_eq!(report.render(), "  a\n  b\n  c");
}

#[test]
fn verbosity_is_scoped_to_the_running_command() {
	assert!(!verbose_output());
	assert!(crate::tests::block_on_in_context(with_verbosity(
		true,
		async { TextTheme::for_stdout().verbose }
	)));
	assert!(!crate::tests::block_on_in_context(with_verbosity(
		false,
		async { verbose_output() }
	)));
	assert!(!TextTheme::for_stdout().verbose);
}

#[test]
fn list_items_keep_multiline_details_aligned() {
	let mut report = TextReport::new(plain());
	report.list(["▲ first\n  |\n1 | x".to_string()], usize::MAX);

	assert_eq!(report.render(), "  ▲ first\n      |\n    1 | x");
}

#[test]
fn nested_tables_indent_further_than_sections() {
	let mut report = TextReport::new(plain());
	report.nested_table(
		&[vec![TableCell::plain("package"), TableCell::plain("core")]],
		4,
	);

	assert_eq!(report.render(), "    package  core");
}

#[test]
fn colored_themes_wrap_each_tone_and_reset_it() {
	let theme = colored();
	assert_eq!(theme.paint("x", Tone::Plain), "x");
	assert_eq!(theme.paint("x", Tone::Heading), "\u{1b}[1mx\u{1b}[0m");
	assert_eq!(theme.paint("x", Tone::Success), "\u{1b}[32;1mx\u{1b}[0m");
	assert_eq!(theme.paint("x", Tone::Warning), "\u{1b}[33;1mx\u{1b}[0m");
	assert_eq!(theme.paint("x", Tone::Error), "\u{1b}[31;1mx\u{1b}[0m");
	assert_eq!(theme.paint("x", Tone::Muted), "\u{1b}[2mx\u{1b}[0m");
	assert_eq!(theme.paint("x", Tone::Accent), "\u{1b}[36mx\u{1b}[0m");

	assert_eq!(theme.paint("x", Tone::Value), "\u{1b}[32mx\u{1b}[0m");
	assert_eq!(plain().paint("x", Tone::Error), "x");

	let mut report = TextReport::new(theme);
	report.headline(Outcome::Failure, "broken", &["1 error".to_string()]);
	assert_eq!(
		report.render(),
		"\u{1b}[31;1m✖\u{1b}[0m \u{1b}[1mbroken\u{1b}[0m\u{1b}[2m · 1 error\u{1b}[0m"
	);
}

#[test]
fn stdout_theme_is_plain_under_test() {
	assert_eq!(TextTheme::for_stdout(), plain());
}

#[test]
fn logs_collapse_to_their_last_line() {
	assert_eq!(summarize_log(""), "");
	assert_eq!(summarize_log("ran `cargo fmt`"), "ran `cargo fmt`");
	assert_eq!(
		summarize_log("Progress: 1\n\n  Done in 2.4s  \n"),
		"Done in 2.4s (+1 earlier lines)"
	);
}

#[test]
fn previews_shorten_long_first_lines() {
	assert_eq!(first_line_preview("", 10), "");
	assert_eq!(first_line_preview("short", 10), "short");
	assert_eq!(first_line_preview("\nshort\n\nmore", 10), "short …");
	assert_eq!(first_line_preview("a long first line", 6), "a long…");
}

#[test]
fn counts_use_the_right_noun() {
	assert_eq!(plural(1, "file", "files"), "1 file");
	assert_eq!(plural(0, "file", "files"), "0 files");
	assert_eq!(display_width("→ é"), 3);
	assert_eq!(display_width("📦 パッケージ"), 13);
}

#[test]
fn tables_align_columns_after_emoji_and_cjk_names() {
	let mut report = TextReport::new(plain());
	report.table(&[
		vec![
			TableCell::plain("sdk"),
			TableCell::plain("v1.1.0"),
			TableCell::plain("cargo"),
		],
		vec![
			TableCell::plain("🚀-launcher"),
			TableCell::plain("v2.0.0"),
			TableCell::plain("npm"),
		],
		vec![
			TableCell::plain("日本語-docs"),
			TableCell::plain("v0.3.1"),
			TableCell::plain("deno"),
		],
	]);
	report.fields(&[
		("名前", "日本語-docs".to_string()),
		("Version", "v0.3.1".to_string()),
	]);

	insta::assert_snapshot!(report.render());
}

#[test]
fn ascii_themes_draw_result_symbols_from_ascii() {
	let mut report = TextReport::new(ascii());
	report.headline(
		Outcome::Success,
		"ok",
		&["2 packages".to_string(), "dry-run".to_string()],
	);
	report.headline(Outcome::Warning, "careful", &[]);
	report.headline(Outcome::Failure, "broken", &[]);
	report.headline(Outcome::Neutral, "note", &[]);
	report.section("Changed files", Some(3));
	report.list(["a.toml", "b.toml", "c.toml"].map(ToString::to_string), 2);

	insta::assert_snapshot!(report.render());
}

#[test]
fn ascii_symbols_are_scoped_to_the_running_command() {
	assert!(!ascii_symbols());
	assert!(crate::tests::block_on_in_context(with_ascii_symbols(
		true,
		async { TextTheme::for_stdout().ascii }
	)));
	assert!(!crate::tests::block_on_in_context(with_ascii_symbols(
		false,
		async { ascii_symbols() }
	)));
	assert!(!TextTheme::for_stdout().ascii);
}

#[test]
fn previews_shorten_wide_text_by_terminal_columns() {
	assert_eq!(first_line_preview("日本語のパッケージ", 6), "日本語…");
	assert_eq!(first_line_preview("日本語のパッケージ", 7), "日本語…");
}

#[test]
fn render_drops_trailing_blank_lines_from_verbatim_blocks() {
	let mut report = TextReport::new(plain());
	report.paragraph("Diff", Tone::Heading);
	report.raw_block("+added\n \n\n");

	assert_eq!(report.render(), "Diff\n\n+added");
}
