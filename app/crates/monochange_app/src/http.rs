//! Page GET routes, server functions, and the static-file fallback.
//!
//! Known pages render the leptoskit shell with status 200. Every other GET
//! falls through to the file handler, which serves site assets or renders the
//! same shell and marks the response 404. That keeps dot-segment aliases such
//! as `/./login` on the login screen while still reporting 404, matching the
//! previous fallback.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::Method;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::patch;
use axum::routing::post;
use axum::routing::put;
use leptos::prelude::LeptosOptions;
use leptos::prelude::provide_context;
use leptos_axum::file_and_error_handler;
use leptos_axum::handle_server_fns_with_context;
use leptos_axum::render_app_to_stream_in_order_with_context;
use server_fn::axum::server_fn_paths;

use crate::app::shell;

/// Paths the file router serves as real pages. Anything else is the 404 fallback.
const PAGES: &[&str] = &[
	"/",
	"/install",
	"/pricing",
	"/changelog",
	"/book",
	"/book/{*chapter}",
	"/login",
	"/auth/callback",
	"/dashboard",
	"/dashboard/{organization}",
	"/dashboard/{organization}/projects/{project}",
	"/dashboard/{organization}/projects/{project}/feedback",
	"/p/{organization}/{project}",
];

/// The SSR router: server functions, known pages, then files and the 404 shell.
pub fn ssr_router(options: LeptosOptions, state: Arc<monochange_app_api::AppState>) -> Router {
	// leptos_routes used to do this. Server rendering spawns blocking resources.
	let _ = any_spawner::Executor::init_tokio();
	let mut router = Router::<LeptosOptions>::new();

	for (path, method) in server_fn_paths() {
		let state = state.clone();
		let handler = move |request: Request<Body>| {
			let state = state.clone();
			async move {
				handle_server_fns_with_context(move || provide_context(state.clone()), request)
					.await
			}
		};
		let route = match method {
			Method::GET => get(handler),
			Method::POST => post(handler),
			Method::PUT => put(handler),
			Method::DELETE => delete(handler),
			Method::PATCH => patch(handler),
			other => panic!("unsupported server function method {other:?}"),
		};
		router = router.route(path, route);
	}

	let context_options = options.clone();
	let shell_options = options.clone();
	let context_state = state.clone();
	let render = render_app_to_stream_in_order_with_context(
		move || {
			provide_context(context_options.clone());
			provide_context(context_state.clone());
		},
		move || shell(shell_options.clone()),
	);

	for path in PAGES {
		let render = render.clone();
		router = router.route(
			path,
			get(move |request: Request<Body>| {
				let render = render.clone();
				async move { render(request).await }
			}),
		);
	}

	let state_for_fallback = state.clone();
	router
		.fallback(file_and_error_handler(move |options: LeptosOptions| {
			provide_context(state_for_fallback.clone());
			provide_context(options.clone());
			shell(options)
		}))
		.with_state(options)
}
