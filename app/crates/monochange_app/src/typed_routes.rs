//! Runtime use of the generated file routes.
//!
//! `src/routes.rs` is the single URL map: the pinned leptoskit router plugin
//! writes it from the files in `src/routes/`. The website renders through
//! Leptos, so this module is where the server uses that map — request-path
//! recognition, the deferred-hydration policy, and the hrefs the book and
//! documentation redirects publish. The browser entry does not link
//! `leptoskit_router`: its native renderer is not the website boundary, and
//! shipping it would grow the eager wasm bundle. See
//! [`crate::routes::_view`].
//!
//! [`Route::parse`] reads a URL the way a browser does, so it accepts escaped
//! spellings the server deliberately answers as 404 (for example `/%6cogin`).
//! [`recognized_path`] therefore also requires the canonical path to match
//! exactly; the middleware and rate-limit aliases keep their behavior.

use leptoskit_router::Routable;

use crate::routes::BookChapter;
use crate::routes::Route;

/// The canonical path and query for `route`.
#[must_use]
pub fn href(route: &Route) -> String {
	route.href().as_str().to_owned()
}

/// The typed `/book` or `/book/*chapter` href.
#[must_use]
pub fn book(chapter: &[String]) -> String {
	href(&Route::BookChapter(BookChapter {
		chapter: chapter.to_vec(),
	}))
}

/// The typed `/book` href.
#[must_use]
pub fn book_root() -> String {
	book(&[])
}

/// The website HTTP paths registered with the SSR router, from the generated
/// file routes.
///
/// The pinned `Pattern::axum_path` writes a trailing catch-all as `/{*name}`,
/// dropping any static prefix. The book route's `[[...chapter]]` is the only
/// catch-all in this tree, so expand its generated entry back to the book root
/// and `/book/{*chapter}`. Registering the raw list would swallow `/pkg` and
/// every other one-segment path.
#[must_use]
pub fn http_paths() -> Vec<String> {
	crate::routes::axum_route_list()
		.into_iter()
		.flat_map(|path| {
			if path == "/{*chapter}" {
				let root = book_root();

				vec![root.clone(), format!("{root}/{{*chapter}}")]
			} else {
				vec![path]
			}
		})
		.collect()
}

/// The parsed route for an absolute path and query, if the file routes own it.
#[must_use]
pub fn parse(url: &str) -> Option<Route> {
	Route::parse(url).ok()
}

/// The route for `path` only when the spelling is its canonical path.
///
/// Escaped aliases and repeated slashes do not match, so callers keep the
/// server's 404 and rate-limit behavior instead of silently normalizing them.
#[must_use]
pub fn recognized_path(path: &str) -> Option<Route> {
	let path = path.split(['?', '#']).next().unwrap_or(path);
	let route = Route::parse(path).ok()?;
	let canonical = route.href();
	let canonical_path = canonical
		.as_str()
		.split(['?', '#'])
		.next()
		.unwrap_or(canonical.as_str());
	let same_path = canonical_path == path
		|| (matches!(route, Route::BookChapter(_)) && canonical_path == path.trim_end_matches('/'));

	same_path.then_some(route)
}

/// Whether a public document hydrates on interaction instead of eagerly.
///
/// These pages are links and buttons only; editable forms, the dashboard, and
/// the sign-in callback keep eager hydration. `/projects` is decided by the
/// caller because its signed-out landing page belongs to this policy.
#[must_use]
pub fn is_deferred_public_path(path: &str) -> bool {
	matches!(
		recognized_path(path),
		Some(
			Route::Index
				| Route::Install
				| Route::Pricing
				| Route::Changelog
				| Route::SignIn(_)
				| Route::BookChapter(_)
		)
	)
}

/// Whether a consent return path is a well-formed typed `/connect` URL.
///
/// The query values keep their own validation in `session.rs`; this only
/// rejects a destination that is not the generated `/connect` route at all.
#[must_use]
pub fn is_connect_return(next: &str) -> bool {
	next.starts_with("/connect?")
		&& !next.contains(['\r', '\n'])
		&& matches!(parse(next), Some(Route::Connect(_)))
}

#[cfg(test)]
#[path = "__tests__/typed_routes_tests.rs"]
mod tests;
