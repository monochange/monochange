use super::*;

#[test]
fn chapter_errors_distinguish_not_found_and_unavailable() {
	let missing = Owner::new().with(|| book_page(Ok(None)).to_html());
	assert!(missing.contains("Chapter not found"));
	let unavailable = Owner::new()
		.with(|| book_page(Err(server_fn::ServerFnError::new("transport error"))).to_html());
	assert!(unavailable.contains("role=\"alert\""));
	assert!(unavailable.contains("unavailable right now"));
}

#[test]
fn rendered_chapter_keeps_site_navigation_and_source_links() {
	let book = crate::book::compiled_book().unwrap();
	let chapter = book.chapter("guide/02-setup").unwrap().clone();
	let page = BookPage {
		chapter,
		navigation: book
			.chapters
			.iter()
			.map(crate::book::chapter_link)
			.collect(),
	};
	let html = Owner::new().with(|| book_page(Ok(Some(page))).to_html());
	assert!(html.contains("Back to monochange.dev"));
	assert!(html.contains("aria-current=\"page\""));
	assert!(
		html.contains(
			"https://github.com/monochange/monochange/blob/main/docs/src/guide/02-setup.md"
		)
	);
	assert!(html.contains("Previous"));
	assert!(html.contains("Next"));
}
