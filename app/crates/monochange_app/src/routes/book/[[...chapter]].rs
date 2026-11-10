//! `/book` and `/book/*chapter`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// A book chapter path. Empty means the book root.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct BookChapter {
	pub chapter: Vec<String>,
}

pub fn page(route: &BookChapter) -> Element {
	crate::pages::book::book_screen(route.chapter.clone())
}
