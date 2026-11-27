//! `/book` and `/book/*chapter`

use leptoskit_router::__private::Element;

/// A book chapter path. Empty means the book root.
#[derive(leptoskit_router::Route, Clone, Debug, PartialEq)]
pub struct BookChapter {
	pub chapter: Vec<String>,
}

pub fn page() -> Element {
	crate::routes::_view::deferred_to_leptos()
}
