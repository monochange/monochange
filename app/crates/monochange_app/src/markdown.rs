//! Formatting for the feedback editor.
//!
//! Feedback is written as Markdown so it maps one-to-one onto provider
//! issues. The editor's toolbar applies a [`MarkdownFormat`] to the current
//! selection; this module does the text work so it can be tested without a
//! browser. Offsets are UTF-16 code units, matching
//! `HTMLTextAreaElement.selectionStart`.

/// A toolbar action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownFormat {
	Bold,
	Italic,
	Code,
	Link,
	Quote,
	BulletList,
	NumberedList,
}

impl MarkdownFormat {
	pub const TOOLBAR: [MarkdownFormat; 7] = [
		MarkdownFormat::Bold,
		MarkdownFormat::Italic,
		MarkdownFormat::Code,
		MarkdownFormat::Link,
		MarkdownFormat::Quote,
		MarkdownFormat::BulletList,
		MarkdownFormat::NumberedList,
	];

	/// The button's accessible name.
	pub fn label(self) -> &'static str {
		match self {
			MarkdownFormat::Bold => "Bold",
			MarkdownFormat::Italic => "Italic",
			MarkdownFormat::Code => "Code",
			MarkdownFormat::Link => "Link",
			MarkdownFormat::Quote => "Quote",
			MarkdownFormat::BulletList => "Bulleted list",
			MarkdownFormat::NumberedList => "Numbered list",
		}
	}

	/// The button's visible glyph.
	pub fn glyph(self) -> &'static str {
		match self {
			MarkdownFormat::Bold => "B",
			MarkdownFormat::Italic => "I",
			MarkdownFormat::Code => "</>",
			MarkdownFormat::Link => "Link",
			MarkdownFormat::Quote => "“ ”",
			MarkdownFormat::BulletList => "•",
			MarkdownFormat::NumberedList => "1.",
		}
	}
}

/// Text after an edit, with the selection to restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedText {
	pub text: String,
	pub selection_start: u32,
	pub selection_end: u32,
}

fn utf16_len(text: &str) -> u32 {
	u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX)
}

/// The byte index of a UTF-16 offset, clamped to the text and snapped back
/// to a character boundary.
fn byte_index(text: &str, utf16_offset: u32) -> usize {
	let mut units = 0;
	for (index, character) in text.char_indices() {
		let width = u32::try_from(character.len_utf16()).unwrap_or(2);
		if units + width > utf16_offset {
			return index;
		}
		units += width;
	}
	text.len()
}

/// Applies `format` to the text between two UTF-16 offsets.
///
/// Inline formats wrap the selection, or insert an empty pair with the caret
/// between. A link wraps the selection as its text and selects the address
/// placeholder. Line formats prefix every line the selection touches.
pub fn apply_format(text: &str, start: u32, end: u32, format: MarkdownFormat) -> FormattedText {
	let (start, end) = (start.min(end), start.max(end));
	let start_byte = byte_index(text, start);
	let end_byte = byte_index(text, end);
	let before = &text[..start_byte];
	let selected = &text[start_byte..end_byte];
	let after = &text[end_byte..];
	let prefix_units = utf16_len(before);

	let inline = |marker: &str| {
		let marker_units = utf16_len(marker);
		FormattedText {
			text: format!("{before}{marker}{selected}{marker}{after}"),
			selection_start: prefix_units + marker_units,
			selection_end: prefix_units + marker_units + utf16_len(selected),
		}
	};

	match format {
		MarkdownFormat::Bold => inline("**"),
		MarkdownFormat::Italic => inline("_"),
		MarkdownFormat::Code => inline("`"),
		MarkdownFormat::Link => {
			let label = if selected.is_empty() {
				"link text"
			} else {
				selected
			};
			let placeholder = "https://";
			let address_start = prefix_units + utf16_len(label) + 3;
			FormattedText {
				text: format!("{before}[{label}]({placeholder}){after}"),
				selection_start: address_start,
				selection_end: address_start + utf16_len(placeholder),
			}
		}
		MarkdownFormat::Quote | MarkdownFormat::BulletList | MarkdownFormat::NumberedList => {
			let line_start = before.rfind('\n').map_or(0, |index| index + 1);
			let line_end = after
				.find('\n')
				.map_or(text.len(), |index| end_byte + index);
			let block = &text[line_start..line_end];
			let prefixed = block
				.split('\n')
				.enumerate()
				.map(|(index, line)| {
					match format {
						MarkdownFormat::Quote => format!("> {line}"),
						MarkdownFormat::BulletList => format!("- {line}"),
						_ => format!("{}. {line}", index + 1),
					}
				})
				.collect::<Vec<_>>()
				.join("\n");
			let block_start = utf16_len(&text[..line_start]);
			FormattedText {
				selection_start: block_start,
				selection_end: block_start + utf16_len(&prefixed),
				text: format!("{}{prefixed}{}", &text[..line_start], &text[line_end..]),
			}
		}
	}
}

/// Renders Markdown to sanitized HTML for previews and portal threads.
#[cfg(not(target_arch = "wasm32"))]
pub fn render(text: &str) -> String {
	use pulldown_cmark::Options;
	use pulldown_cmark::Parser;

	let mut html = String::new();
	pulldown_cmark::html::push_html(
		&mut html,
		Parser::new_ext(
			text,
			Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
		),
	);
	ammonia::Builder::default()
		.link_rel(Some("nofollow noopener noreferrer"))
		.clean(&html)
		.to_string()
}

#[cfg(test)]
#[path = "__tests__/markdown_tests.rs"]
mod tests;
