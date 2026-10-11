use rstest::rstest;

use super::FormattedText;
use super::MarkdownFormat;
use super::apply_format;

fn formatted(text: &str, start: u32, end: u32) -> FormattedText {
	FormattedText {
		text: text.to_owned(),
		selection_start: start,
		selection_end: end,
	}
}

#[rstest]
#[case(MarkdownFormat::Bold, "Make **this** bold", 7, 11)]
#[case(MarkdownFormat::Italic, "Make _this_ bold", 6, 10)]
#[case(MarkdownFormat::Code, "Make `this` bold", 6, 10)]
fn inline_formats_wrap_the_selection(
	#[case] format: MarkdownFormat,
	#[case] text: &str,
	#[case] start: u32,
	#[case] end: u32,
) {
	assert_eq!(
		apply_format("Make this bold", 5, 9, format),
		formatted(text, start, end)
	);
}

#[rstest]
fn empty_selections_place_the_caret_between_markers() {
	assert_eq!(
		apply_format("Hi ", 3, 3, MarkdownFormat::Bold),
		formatted("Hi ****", 5, 5)
	);
}

#[rstest]
fn reversed_and_out_of_range_selections_are_normalized() {
	assert_eq!(
		apply_format("abc", 99, 1, MarkdownFormat::Code),
		formatted("a`bc`", 2, 4)
	);
}

#[rstest]
fn links_select_the_address_placeholder() {
	assert_eq!(
		apply_format("See docs here", 4, 8, MarkdownFormat::Link),
		formatted("See [docs](https://) here", 11, 19)
	);
	assert_eq!(
		apply_format("", 0, 0, MarkdownFormat::Link),
		formatted("[link text](https://)", 12, 20)
	);
}

#[rstest]
fn offsets_count_utf16_units_like_the_browser() {
	// "🎉" is two UTF-16 units; the selection covers "é🎉".
	let result = apply_format("Café🎉 time", 3, 6, MarkdownFormat::Bold);
	assert_eq!(result.text, "Caf**é🎉** time");
	assert_eq!((result.selection_start, result.selection_end), (5, 8));
	// An offset inside a surrogate pair snaps back to the character start.
	assert_eq!(apply_format("🎉", 1, 1, MarkdownFormat::Code).text, "``🎉");
}

#[rstest]
#[case(MarkdownFormat::Quote, "intro\n> one\n> two\nend")]
#[case(MarkdownFormat::BulletList, "intro\n- one\n- two\nend")]
#[case(MarkdownFormat::NumberedList, "intro\n1. one\n2. two\nend")]
fn line_formats_prefix_every_touched_line(#[case] format: MarkdownFormat, #[case] text: &str) {
	// The selection starts mid-"one" and ends mid-"two".
	let result = apply_format("intro\none\ntwo\nend", 7, 11, format);
	assert_eq!(result.text, text);
	assert_eq!(result.selection_start, 6);
	assert_eq!(result.selection_end, u32::try_from(text.len() - 4).unwrap());
}

#[rstest]
fn line_formats_work_on_the_last_line() {
	assert_eq!(
		apply_format("only", 0, 0, MarkdownFormat::Quote),
		formatted("> only", 0, 6)
	);
}

#[rstest]
fn every_toolbar_button_has_a_name_and_glyph() {
	for format in MarkdownFormat::TOOLBAR {
		assert!(!format.label().is_empty());
		assert!(!format.glyph().is_empty());
	}
	assert_eq!(MarkdownFormat::BulletList.label(), "Bulleted list");
}

#[cfg(not(target_arch = "wasm32"))]
#[rstest]
fn previews_are_sanitized_markdown() {
	let html = super::render(
		"**Bold** and [link](https://example.com)\n\n<script>alert(1)</script><img src=x onerror=alert(1)>",
	);
	assert!(html.contains("<strong>Bold</strong>"));
	assert!(html.contains("rel=\"nofollow noopener noreferrer\""));
	assert!(!html.contains("<script"));
	assert!(!html.contains("onerror"));
}
