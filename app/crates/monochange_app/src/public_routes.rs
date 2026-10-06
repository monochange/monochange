//! Redirect legacy documentation bookmarks to the published book.

use axum::Router;
use axum::response::Redirect;
use axum::routing::get;

use crate::links::BOOK_URL;

/// Documentation destinations served without authentication or application state.
pub fn book_redirects() -> Router {
	Router::new()
		.route("/docs", get(|| async { Redirect::permanent(BOOK_URL) }))
		.route("/docs/", get(|| async { Redirect::permanent(BOOK_URL) }))
		.route(
			"/docs/{*chapter}",
			get(|| async { Redirect::permanent(BOOK_URL) }),
		)
}

#[cfg(test)]
#[path = "__tests__/public_routes_tests.rs"]
mod tests;
