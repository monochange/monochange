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
pub mod markdown;

pub mod pages;
pub mod projects;
#[cfg(not(target_arch = "wasm32"))]
pub mod public_routes;
#[cfg(not(target_arch = "wasm32"))]
pub mod rate_limit;
pub mod repository_filter;
pub mod server_fns;

pub use app::App;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

/// Hydrate the Leptos app on the client (WASM only).
///
/// leptoskit SSR apps call this from `HydrationScripts` the same way. Once
/// `leptoskit_rpc` / web is linked (Leptos 0.9 bump), configure the RPC client
/// here with `leptoskit_rpc::client::configure(ClientConfig::new())` before
/// mounting so product RPCs share one origin with Class B Axum routes.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
	console_error_panic_hook::set_once();
	_ = console_log::init_with_level(log::Level::Debug);

	// Lazy routes hydrate through their own chunk-loading mount; hydrate_body
	// panics when it meets one. leptoskit analytics uses hydrate_body when
	// there are no `#[lazy_route]` boundaries.
	leptos::mount::hydrate_lazy(app::App);
}
