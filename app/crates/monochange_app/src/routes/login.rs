//! `/login`

use leptoskit_router::__private::Element;

/// The sign-in screen, remembering where to return after GitHub.
#[derive(leptoskit_router::Route, Clone, Debug, PartialEq)]
pub struct SignIn {
	pub next: Option<String>,
}

pub fn page() -> Element {
	crate::routes::_view::deferred_to_leptos()
}
