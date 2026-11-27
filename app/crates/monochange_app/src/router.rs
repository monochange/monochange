//! The production HTTP router, shared by the server binary and its tests.
//!
//! One router composes both browser frontends behind the same backend: the
//! website entry and the projects entry. The route listings of both trees
//! merge before a single `leptos_routes_with_context` call, so the
//! server-function `/api` routes register exactly once and a path
//! dispatcher picks the matching SSR route tree per request.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn;
use axum::middleware::from_fn_with_state;
use axum::response::Json;
use axum::routing::get;
use leptos::prelude::LeptosOptions;
use leptos::prelude::provide_context;
use leptos_axum::AxumRouteListing;
use leptos_axum::LeptosRoutes;
use leptos_axum::generate_route_list;
use monochange_app_api::AppState;
use serde::Serialize;

/// The website listings discovered from the Leptos route tree.
///
/// Discovery suppresses resource loads process-wide while it runs, so memoize
/// it: production discovers once at startup, and parallel SSR tests never race
/// the suppression against another render.
fn discovered_website_routes() -> Vec<AxumRouteListing> {
	static DISCOVERED: std::sync::OnceLock<Vec<AxumRouteListing>> = std::sync::OnceLock::new();
	DISCOVERED
		.get_or_init(|| generate_route_list(crate::app::App))
		.clone()
}

/// The website listings the generated file routes govern.
///
/// The generated `axum_route_list()` owns which website paths register. For
/// each one, reuse the metadata the Leptos tree discovered (`SsrMode`,
/// methods, and regeneration), so Suspense streaming and lazy-route preloads
/// are unchanged. A generated path missing from discovery still registers
/// with the toolkit's default listing, and the contract test fails.
fn website_routes_from(discovered: &[AxumRouteListing]) -> Vec<AxumRouteListing> {
	let by_path = discovered
		.iter()
		.map(|listing| (listing.path(), listing))
		.collect::<HashMap<_, _>>();

	leptoskit_router::ssr::route_listings(crate::typed_routes::http_paths())
		.into_iter()
		.map(|listing| {
			by_path
				.get(listing.path())
				.map_or(listing, |discovered| (*discovered).clone())
		})
		.collect()
}

/// The SSR route listings of both browser frontends.
#[must_use]
pub fn routes() -> Vec<AxumRouteListing> {
	static ROUTES: std::sync::OnceLock<Vec<AxumRouteListing>> = std::sync::OnceLock::new();
	ROUTES
		.get_or_init(|| {
			let mut routes = website_routes_from(&discovered_website_routes());
			routes.extend(generate_route_list(
				monochange_projects_web::app::ProjectsApp,
			));
			routes
		})
		.clone()
}

#[derive(Serialize)]
struct HealthResponse {
	status: &'static str,
	http: &'static str,
	version: &'static str,
}

/// Builds the production router: health and machine APIs, the SSR routes of
/// both frontends on one registration, the static file handler, and the
/// auth/rate-limit middleware stack.
pub fn build_router(
	state: &Arc<AppState>,
	options: LeptosOptions,
	rate_limit: crate::rate_limit::RateLimit,
) -> Router {
	// Route discovery temporarily suppresses resource loads process-wide.
	// Match production startup: discover once before any request can run.
	let routes = routes();

	let ssr_router = Router::<LeptosOptions>::new()
		.leptos_routes_with_context(
			&options,
			routes,
			{
				let options = options.clone();
				let state = state.clone();
				move || {
					provide_context(options.clone());
					provide_context(state.clone());
				}
			},
			{
				let options = options.clone();
				move || crate::app::shell(options.clone())
			},
		)
		.fallback(leptos_axum::file_and_error_handler({
			let state = state.clone();
			move |options: LeptosOptions| {
				provide_context(options.clone());
				provide_context(state.clone());
				crate::app::shell(options)
			}
		}))
		.with_state(options);

	Router::<()>::new()
		.merge(crate::public_routes::book_redirects())
		.route(
			"/health",
			get(|| {
				async move {
					Json(HealthResponse {
						status: "ok",
						version: env!("CARGO_PKG_VERSION"),
						http: "up",
					})
				}
			}),
		)
		// Class B machine APIs (webhooks, hosted release OIDC, embed SDK, tasks).
		.merge(monochange_backend::machine_api_router(
			state.as_ref().clone(),
		))
		.merge(ssr_router)
		// Hosted release RPC operations authenticate their workflow before
		// the body is read; the gate ignores every other path.
		.layer(from_fn_with_state(
			state.clone(),
			crate::server_fns::hosted::machine_gate,
		))
		// Embedded feedback SDK and worker RPC operations: per-project CORS.
		.layer(from_fn_with_state(
			state.clone(),
			crate::server_fns::feedback_sdk::sdk_gate,
		))
		// Client sign-in RPC operations: handoff body limit and no-store.
		.layer(from_fn(crate::server_fns::client_handoff::handoff_gate))
		.layer(from_fn(crate::public_routes::legacy_projects))
		// The gate runs after the principal middleware, so a client token is
		// already in request extensions when its path is checked.
		.layer(from_fn(crate::server_fns::session::bearer_gate))
		.layer(from_fn_with_state(
			state.clone(),
			monochange_app_api::sessions::principal_middleware,
		))
		.layer(from_fn(crate::public_routes::pkg_cache_control))
		.layer(from_fn_with_state(rate_limit, crate::rate_limit::limit))
}

#[cfg(test)]
#[path = "__tests__/router_tests.rs"]
mod tests;
