use super::*;

static FIXTURE: include_dir::Dir<'_> =
	include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../../fixtures/tests/website-book");

#[test]
fn compiled_book_renders_safe_chapters_with_working_navigation() {
	let book = Book::read(&FIXTURE).unwrap();
	let first = book.chapter("guide/first").unwrap();
	assert_eq!(first.title, "First release");
	assert_eq!(first.previous.as_ref().unwrap().href, "/book");
	assert_eq!(first.next.as_ref().unwrap().href, "/book/guide/next");
	assert!(first.html.contains("id=\"see-the-plan\""));
	assert!(first.html.contains("id=\"see-the-plan-1\""));
	assert!(first.html.contains("class=\""));
	assert!(first.html.contains("storage type function rust"));
	assert!(first.html.contains("href=\"/book/guide/next\""));
	assert!(first.html.contains("href=\"/book\""));
	assert!(first.html.contains("href=\"https://example.com/\""));
	assert!(
		first
			.html
			.contains("&lt;example&gt;safe &amp; visible&lt;/example&gt;")
	);
	let introduction = book.chapter("").unwrap();
	assert!(introduction.html.contains("/book-assets/branding/mark.svg"));
	assert!(!introduction.html.contains("<script"));
	assert!(!introduction.html.contains("javascript:"));
	assert!(!introduction.html.contains("onclick"));
	assert!(
		introduction
			.html
			.contains("src=\"/book-assets/branding/mark.svg\"")
	);
	assert!(introduction.html.contains("href=\"/book/guide/next\""));
	assert!(!introduction.html.contains("//[invalid"));
	assert!(book.chapter("missing").is_none());
}

#[test]
fn real_book_has_all_listed_chapters_and_no_markdown_links() {
	let book = Book::read(&BOOK_FILES).unwrap();
	assert!(book.chapters.len() > 40);
	for chapter in &book.chapters {
		assert!(!chapter.html.contains("href=\"../"));
		assert!(!chapter.html.contains("href=\"./"));
	}
	assert!(
		book.chapter("")
			.unwrap()
			.html
			.contains("src=\"/book-assets/branding/logo-280.png\"")
	);
}

#[test]
fn invalid_books_fail_instead_of_serving_empty_content() {
	assert!(Book::read(FIXTURE.get_dir("branding").unwrap()).is_err());
	assert_eq!(
		Book::read(FIXTURE.get_dir("empty").unwrap()).err().unwrap(),
		"book has no chapters"
	);
	assert!(
		Book::read(FIXTURE.get_dir("missing").unwrap())
			.err()
			.unwrap()
			.contains("missing.md")
	);
}

#[test]
fn links_preserve_anchors_queries_and_external_destinations() {
	let book = Book::read(&FIXTURE).unwrap();
	let first = book.chapter("guide/first").unwrap();
	assert!(first.html.contains("href=\"#see-the-plan\""));
	assert!(
		first
			.html
			.contains("href=\"/book/guide/next?from=first#next-steps\"")
	);
	assert!(first.html.contains("href=\"https://example.com/guide\""));
	assert!(first.html.contains("id=\"under_score-and-punctuation\""));
	assert!(book.chapter("guide/next").unwrap().html.contains(
		"https://github.com/monochange/monochange/blob/main/crates/monochange/src/cli.rs"
	));
}

#[test]
fn highlighting_errors_identify_the_chapter_instead_of_hiding_the_failure() {
	let grammar = syntect::parsing::SyntaxDefinition::load_from_str(
		read_file(&FIXTURE, "broken.sublime-syntax").unwrap(),
		true,
		None,
	)
	.unwrap();
	let mut syntax = syntect::parsing::SyntaxSetBuilder::new();
	syntax.add(grammar);
	let error = render_markdown(
		read_file(&FIXTURE, "highlighting-error.md").unwrap(),
		"highlighting-error.md",
		&syntax.build(),
	)
	.unwrap_err();
	assert!(error.contains("book code highlighting failed in highlighting-error.md"));
}
