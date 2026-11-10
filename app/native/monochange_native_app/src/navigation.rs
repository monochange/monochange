//! One in-memory stack for the native guest. Cloning the router shares history.

use std::sync::OnceLock;

use leptoskit_router::Routable;

use crate::routes::Navigation;
use crate::routes::Route;

#[cfg(test)]
#[path = "__tests__/navigation_tests.rs"]
mod tests;

/// The guest's router. The first call starts on the project floor.
pub fn navigation() -> Navigation {
	static ROUTER: OnceLock<Navigation> = OnceLock::new();
	ROUTER
		.get_or_init(|| Navigation::new(Route::parse("/").expect("the floor route parses")))
		.clone()
}
