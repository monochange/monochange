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
/// leptoskit SSR apps call this from `HydrationScripts`. Product UI RPCs stay
/// on Leptos `#[server]` / `#[api]` HTTP (`POST /api/<endpoint>`); this binary
/// does not mount leptoskit_rpc's `/live` WebSocket or SSE transport, so hydrate
/// must not call `leptoskit_rpc::client::configure` — that starts `LiveClient`,
/// which reconnects to `/live` forever and floods the console with 404s
/// (Lighthouse budget failures on every public route).
///
/// Install the wasm-bindgen executor before `hydrate_lazy` so any
/// `spawn_local` from the mount path has a runtime.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
	console_error_panic_hook::set_once();
	_ = console_log::init_with_level(log::Level::Debug);
	// Err(AlreadySet) is fine if something else installed it first.
	_ = any_spawner::Executor::init_wasm_bindgen();

	// Lazy routes hydrate through their own chunk-loading mount; hydrate_body
	// panics when it meets one.
	leptos::mount::hydrate_lazy(app::App);
}
