//! `/auth/callback`

use leptoskit_router::__private::Element;

/// The OAuth callback query.
#[derive(leptoskit_router::Route, Clone, Debug, PartialEq)]
pub struct AuthCallback {
	pub code: Option<String>,
	pub state: Option<String>,
}

pub fn page() -> Element {
	crate::routes::_view::deferred_to_leptos()
}
