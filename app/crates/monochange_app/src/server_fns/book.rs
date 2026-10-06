//! Book content delivered by the server to the shared website shell.

use leptos::server;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterLink {
	pub title: String,
	pub href: String,
	pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookChapter {
	pub title: String,
	pub href: String,
	pub source: String,
	pub section: String,
	/// Generated and sanitized on the server.
	pub html: String,
	pub previous: Option<ChapterLink>,
	pub next: Option<ChapterLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookPage {
	pub chapter: BookChapter,
	pub navigation: Vec<ChapterLink>,
}

/// Read a chapter from the book shipped with this app build.
// Leptos server functions require an async signature.
#[allow(clippy::unused_async)]
#[server]
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
