use super::*;

fn ascii() -> TextTheme {
	TextTheme {
		color: false,
		verbose: false,
		ascii: true,
	}
}

#[test]
fn previews_shorten_long_first_lines() {
	assert_eq!(first_line_preview("", 10), "");
	assert_eq!(first_line_preview("short", 10), "short");
	assert_eq!(first_line_preview("\nshort\n\nmore", 10), "short …");
	assert_eq!(first_line_preview("a long first line", 6), "a long…");
}

#[test]
fn ascii_themes_draw_result_symbols_from_ascii() {
	let mut report = TextReport::new(ascii());
	report.summary(
		Outcome::Success,
		"ok",
		&["2 packages".to_string(), "dry-run".to_string()],
	);
	report.summary(Outcome::Warning, "careful", &[]);
	report.summary(Outcome::Failure, "broken", &[]);
	report.summary(Outcome::Neutral, "note", &[]);
	report.section("Changed files", Some(3));
	report.list(["a.toml", "b.toml", "c.toml"].map(ToString::to_string), 2);

	insta::assert_snapshot!(report.render());
}
