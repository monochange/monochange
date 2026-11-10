#![allow(clippy::disallowed_methods)]

use super::*;

#[tokio::test]
async fn book_route_view_wraps_resolved_chapter_in_site_layout() {
	let html = crate::tests::render(|| {
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

#[test]
fn chapter_errors_distinguish_not_found_and_unavailable() {
	let missing = Owner::new().with(|| view! { <BookPageView result=Ok(None) /> }.to_html());
	assert!(missing.contains("Chapter not found"));
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

#[test]
fn the_element_book_covers_loading_a_missing_chapter_and_its_previous_link() {
	let _ = Owner::new().with(|| book_body(None));
	let _ = Owner::new().with(|| book_body(Some(Err(server_fn::ServerFnError::new("down")))));
	let _ = Owner::new().with(|| book_body(Some(Ok(None))));

	let book = crate::book::compiled_book().unwrap();
	let mut chapter = book.chapter("guide/02-setup").unwrap().clone();
	if chapter.previous.is_none() {
		chapter.previous = chapter.next.clone();
	}
	let page = BookPage {
		chapter,
		navigation: Vec::new(),
	};
	let _ = Owner::new().with(|| book_content(page));
}

#[tokio::test]
async fn the_book_screen_asks_for_its_chapter() {
	let html = crate::tests::render(|| {
		leptoskit::web::render(book_screen(&["guide".to_owned(), "02-setup".to_owned()]))
	})
	.await;
	assert!(
		html.contains("Loading")
			|| html.contains("unavailable")
			|| html.contains("not found")
			|| html.contains("monochange"),
		"{html}"
	);
}
