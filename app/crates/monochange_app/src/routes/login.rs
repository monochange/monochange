//! `/login`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// Sign-in. `next` is where a successful sign-in returns.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct SignIn {
	pub next: Option<String>,
}

pub fn page(route: &SignIn) -> Element {
	crate::pages::login::login_screen(route.next.as_deref())
}
