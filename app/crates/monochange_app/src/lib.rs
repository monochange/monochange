//! `monochange_app` — WASM client entrypoint.
//!
//! This is compiled to WebAssembly and hydrates the Leptos app
//! on the client side for interactivity after SSR.

// Leptos' generated view types exceed Rust's default depth during release builds.
#![recursion_limit = "256"]

pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod book;
pub mod color_mode;
pub mod components;
pub mod error;
pub mod links;
pub use monochange_projects::markdown;

pub mod pages;
pub use monochange_projects::projects;
#[cfg(not(target_arch = "wasm32"))]
pub mod public_routes;
#[cfg(not(target_arch = "wasm32"))]
pub mod rate_limit;
pub use monochange_projects::repository_filter;
pub mod server_fns;

pub use app::App;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

/// Hydrate the Leptos app on the client (WASM only).
///
/// leptoskit SSR apps call this from `HydrationScripts`. Product RPCs share
/// the page origin with Class B Axum routes via the leptoskit_rpc web client.
///
/// RPC calls use the browser's origin by default. Do not configure the live
/// client until the server provides its live endpoint.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
	console_error_panic_hook::set_once();
	_ = console_log::init_with_level(log::Level::Debug);
	_ = any_spawner::Executor::init_wasm_bindgen();

	// Lazy routes hydrate through their own chunk-loading mount; hydrate_body
	// panics when it meets one.
	leptos::mount::hydrate_lazy(app::App);
}
