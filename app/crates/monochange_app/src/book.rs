//! Compiled Markdown book, rendered within the website.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::CodeBlockKind;
use pulldown_cmark::Event;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;
use syntect::html::ClassStyle;
use syntect::html::ClassedHTMLGenerator;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub use crate::server_fns::book::BookChapter;
pub use crate::server_fns::book::ChapterLink;

pub static BOOK_FILES: include_dir::Dir<'_> =
	include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../../docs/src");

/// The canonical table of contents and its rendered chapters.
pub struct Book {
	pub chapters: Vec<BookChapter>,
}

impl Book {
	/// Validate and render every chapter named by the book's table of contents.
	pub fn read(files: &include_dir::Dir<'_>) -> Result<Self, String> {
		let summary = read_file(files, "SUMMARY.md")?;
		let syntax = SyntaxSet::load_defaults_newlines();
		let mut section = "Overview".to_owned();
		let mut chapters = Vec::new();
		let mut events = Parser::new(summary);
		while let Some(event) = events.next() {
			match event {
				Event::Start(Tag::Heading { .. }) => {
					let title = plain_text(
						events
							.by_ref()
							.take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_)))),
					);
					if title != "Summary" {
						section = title;
					}
				}
				Event::Start(Tag::Link { dest_url, .. }) => {
					let title = plain_text(
						events
							.by_ref()
							.take_while(|event| !matches!(event, Event::End(TagEnd::Link))),
					);
					let source = dest_url.trim_start_matches("./").to_owned();
					let markdown = read_file(files, &source)?;
					chapters.push(BookChapter {
						title,
						href: chapter_href(&source),
						html: render_markdown(markdown, &source, &syntax)?,
						source,
						section: section.clone(),
						previous: None,
						next: None,
					});
				}
				_ => {}
			}
		}
		if chapters.is_empty() {
			return Err("book has no chapters".to_owned());
		}
		let links = chapters.iter().map(chapter_link).collect::<Vec<_>>();
		for (index, chapter) in chapters.iter_mut().enumerate() {
			chapter.previous = index
				.checked_sub(1)
				.and_then(|previous| links.get(previous))
				.cloned();
			chapter.next = links.get(index + 1).cloned();
		}
		Ok(Self { chapters })
	}

	/// Resolve only chapters listed in the table of contents.
	pub fn chapter(&self, slug: &str) -> Option<&BookChapter> {
		let href = if slug.is_empty() {
			"/book".to_owned()
		} else {
			format!("/book/{slug}")
		};
		self.chapters.iter().find(|chapter| chapter.href == href)
	}
}

/// Render once per server process; source files are embedded in the binary.
pub fn compiled_book() -> Result<&'static Book, String> {
	static BOOK: OnceLock<Result<Book, String>> = OnceLock::new();
	BOOK.get_or_init(|| Book::read(&BOOK_FILES))
		.as_ref()
		.map_err(Clone::clone)
}

pub(crate) fn chapter_link(chapter: &BookChapter) -> ChapterLink {
	ChapterLink {
		title: chapter.title.clone(),
		href: chapter.href.clone(),
		section: chapter.section.clone(),
	}
}

fn read_file<'a>(files: &'a include_dir::Dir<'_>, path: &str) -> Result<&'a str, String> {
	files
		.get_file(files.path().join(path))
		.and_then(include_dir::File::contents_utf8)
		.ok_or_else(|| format!("book file is missing or is not UTF-8: {path}"))
}

pub(crate) fn chapter_href(source: &str) -> String {
	let path = std::path::Path::new(source).with_extension("");
	let slug = path.to_string_lossy();
	if slug == "readme" || slug == "index" {
		"/book".to_owned()
	} else {
		format!("/book/{slug}")
	}
}

fn plain_text<'a>(events: impl Iterator<Item = Event<'a>>) -> String {
	events
		.filter_map(|event| {
			match event {
				Event::Text(text) | Event::Code(text) => Some(text.into_string()),
				_ => None,
			}
		})
		.collect()
}

fn render_markdown(markdown: &str, source: &str, syntax: &SyntaxSet) -> Result<String, String> {
	let mut rendered = Vec::new();
	let mut headings = HashMap::<String, usize>::new();
	let mut events = Parser::new_ext(
		markdown,
		Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
	);
	while let Some(event) = events.next() {
		match event {
			Event::Start(Tag::Heading { level, .. }) => {
				let contents = events
					.by_ref()
					.take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_))))
					.collect::<Vec<_>>();
				let text = plain_text(contents.iter().cloned());
				let slug = text
					.to_lowercase()
					.chars()
					.filter_map(|character| {
						if character.is_alphanumeric() || character == '_' || character == '-' {
							Some(character)
						} else if character.is_whitespace() {
							Some('-')
						} else {
							None
						}
					})
					.collect::<String>();
				let count = headings.entry(slug.clone()).or_default();
				let id = if *count == 0 {
					slug.clone()
				} else {
					format!("{slug}-{count}")
				};
				*count += 1;
				rendered.push(Event::Start(Tag::Heading {
					level,
					id: Some(id.into()),
					classes: Vec::new(),
					attrs: Vec::new(),
				}));
				rendered.extend(contents);
				rendered.push(Event::End(TagEnd::Heading(level)));
			}
			Event::Start(Tag::CodeBlock(kind)) => {
				let language = match kind {
					CodeBlockKind::Fenced(language) => language.into_string(),
					CodeBlockKind::Indented => String::new(),
				};
				let code = plain_text(
					events
						.by_ref()
						.take_while(|event| !matches!(event, Event::End(TagEnd::CodeBlock))),
				);
				let language = language.split(',').next().unwrap_or_default();
				let token = match language {
					"bash" | "shell" | "console" => "sh",
					language => language,
				};
				let grammar = syntax
					.find_syntax_by_token(token)
					.unwrap_or_else(|| syntax.find_syntax_plain_text());
				let mut generator =
					ClassedHTMLGenerator::new_with_class_style(grammar, syntax, ClassStyle::Spaced);
				for line in LinesWithEndings::from(&code) {
					generator
						.parse_html_for_line_which_includes_newline(line)
						.map_err(|error| {
							format!("book code highlighting failed in {source}: {error}")
						})?;
				}
				rendered.push(Event::Html(
					format!("<pre><code>{}</code></pre>", generator.finalize()).into(),
				));
			}
			event => rendered.push(event),
		}
	}
	let mut html = String::new();
	pulldown_cmark::html::push_html(&mut html, rendered.into_iter());
	Ok(ammonia::Builder::default()
		.add_generic_attributes(&["id", "class"])
		// Rewrite both Markdown URLs and URLs in embedded HTML. Invalid URLs are removed.
		.url_relative(ammonia::UrlRelative::Custom(Box::new(BookLinks {
			base: url::Url::parse(&format!(
				"https://github.com/monochange/monochange/blob/main/docs/src/{source}"
			))
			.unwrap_or_else(|error| panic!("book source has an invalid HTTPS URL: {error}")),
		})))
		.clean(&html)
		.to_string())
}

struct BookLinks {
	base: url::Url,
}

impl ammonia::UrlRelativeEvaluate<'_> for BookLinks {
	fn evaluate<'url>(&self, destination: &'url str) -> Option<Cow<'url, str>> {
		if destination.starts_with('#') {
			return Some(Cow::Borrowed(destination));
		}
		let target = self.base.join(destination).ok()?;
		if target.origin() != self.base.origin() {
			return Some(Cow::Owned(target.to_string()));
		}
		let Some(path) = target
			.path()
			.strip_prefix("/monochange/monochange/blob/main/docs/src/")
		else {
			return Some(Cow::Owned(target.to_string()));
		};
		let is_chapter = std::path::Path::new(path)
			.extension()
			.and_then(|extension| extension.to_str())
			.is_some_and(|extension| {
				extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("html")
			});
		let mut href = if is_chapter {
			chapter_href(path)
		} else {
			format!("/book-assets/{path}")
		};
		if let Some(query) = target.query() {
			href.push('?');
			href.push_str(query);
		}
		if let Some(fragment) = target.fragment() {
			href.push('#');
			href.push_str(fragment);
		}
		Some(Cow::Owned(href))
	}
}

#[cfg(test)]
#[path = "__tests__/book_tests.rs"]
mod tests;
