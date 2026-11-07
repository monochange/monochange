//! Book content delivered by the server to the shared website shell.

use leptos::server;
pub use monochange_shared::BookChapter;
pub use monochange_shared::BookPage;
pub use monochange_shared::ChapterLink;

/// Read a chapter from the book shipped with this app build.
// Leptos server functions require an async signature.
#[allow(clippy::unused_async)]
#[server(endpoint = "get_book_page")]
pub async fn get_book_page(chapter: String) -> Result<Option<BookPage>, server_fn::ServerFnError> {
	let book = crate::book::compiled_book().map_err(server_fn::ServerFnError::new)?;
	Ok(book.chapter(&chapter).map(|chapter| {
		BookPage {
			chapter: chapter.clone(),
			navigation: book
				.chapters
				.iter()
				.map(crate::book::chapter_link)
				.collect(),
		}
	}))
}
