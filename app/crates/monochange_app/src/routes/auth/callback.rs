//! `/auth/callback`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// GitHub's return to the site.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct AuthCallback {
	pub code: Option<String>,
	pub state: Option<String>,
}

pub fn page(route: &AuthCallback) -> Element {
	crate::pages::login::callback_screen(route.code.clone(), route.state.clone())
}
