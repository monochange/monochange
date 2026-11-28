//! `/connect`

use leptoskit_router::__private::Element;

/// A client consent request, forwarded exactly as the client sent it.
#[derive(leptoskit_router::Route, Clone, Debug, PartialEq)]
pub struct Connect {
	pub client: Option<String>,
	pub redirect_uri: Option<String>,
	pub code_challenge: Option<String>,
	pub state: Option<String>,
	pub code_challenge_method: Option<String>,
}

pub fn page() -> Element {
	crate::routes::_view::deferred_to_leptos()
}
