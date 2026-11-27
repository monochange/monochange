//! `/dashboard`

use leptoskit_router::__private::Element;

/// The workspace, filtered by `q` and `org`.
#[derive(leptoskit_router::Route, Clone, Debug, PartialEq)]
pub struct Workspace {
	pub q: Option<String>,
	pub org: Option<String>,
}

pub fn page() -> Element {
	crate::routes::_view::deferred_to_leptos()
}
