//! One file-router stack for the document.
//!
//! The server builds it from the request URL leptos already parsed (so `.`
//! and `..` collapse when the request has a host). The browser builds it
//! from `location` once, and later navigations push that same stack.

use leptoskit_router::Routable;

use crate::routes::Navigation;
use crate::routes::Route;

/// The router for this document.
pub fn current() -> Navigation {
	#[cfg(all(target_arch = "wasm32", feature = "hydrate"))]
	{
		Navigation::from_browser().unwrap_or_else(|_| Navigation::new(home()))
	}
	#[cfg(not(any(not(target_arch = "wasm32"), feature = "hydrate")))]
	{
		Navigation::new(home())
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		Navigation::new(route_from_request())
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn route_from_request() -> Route {
	use leptos::prelude::use_context;
	use leptos_router::location::RequestUrl;

	let parsed = use_context::<RequestUrl>().and_then(|url| url.parse().ok());
	let path = parsed.as_ref().map(|url| url.path()).unwrap_or("/");
	let search = parsed.as_ref().map(|url| url.search()).unwrap_or("");
	// Percent-encoded letters and empty segments (`//login`, `/%6cogin`) are
	// not the OAuth pages. The file router would decode them into `/login`.
	// Dot segments are already collapsed by RequestUrl when the request has a
	// host, which is what the alias rate-limit tests rely on.
	if path.contains('%') || path.contains("//") {
		return crate::routes::Route::NotFound(leptoskit_router::NotFound {
			path: path
				.split('/')
				.filter(|segment| !segment.is_empty())
				.map(str::to_owned)
				.collect(),
		});
	}
	let href = if search.is_empty() {
		path.to_owned()
	} else {
		format!("{path}?{search}")
	};

	Route::parse(&href).unwrap_or_else(|_| home())
}

fn home() -> Route {
	Route::parse("/").unwrap_or_else(|_| unreachable!("the home route is registered"))
}
