#![allow(clippy::disallowed_methods)]

use super::*;

#[tokio::test]
async fn book_route_view_wraps_resolved_chapter_in_site_layout() {
	// Rendered inside a router, as production book pages are: the not-found
	// branch renders the shared error page, which reads the location.
	let html = crate::tests::render_at("/book", || {
		let chapter = Resource::new_blocking(|| (), |()| async { Ok::<_, ServerFnError>(None) });
		book_route_view(chapter)
	})
	.await;
	assert!(html.contains("book-layout"), "{html}");
	assert!(
		html.contains("Chapter not found") || html.contains("Loading the book"),
		"{html}"
	);
}

#[tokio::test]
async fn chapter_errors_distinguish_not_found_and_unavailable() {
	// The not-found branch renders the shared error template inside a router.
	let missing = crate::tests::render_at("/book", || {
		view! { <BookPageView result=Ok(None) /> }
	})
	.await;
	assert!(missing.contains("Chapter not found"), "{missing}");
	let unavailable = Owner::new().with(|| {
		view! { <BookPageView result=Err(server_fn::ServerFnError::new("transport error")) /> }
			.to_html()
	});
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
	let html = Owner::new().with(|| view! { <BookPageView result=Ok(Some(page)) /> }.to_html());
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
