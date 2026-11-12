//! monochange projects browser entry: the product route tree and its
//! hydration.
//!
//! This crate is the projects frontend's `cdylib`: `app/projects/leptoskit.toml`
//! selects it as the browser package, so `pkg/projects/` carries its entry
//! bundle and the `hydrate` wasm-bindgen export lives only here. The product
//! pages and route tree live in this entry crate — not in the shared
//! `monochange_projects` library — because `#[lazy_route]` emits
//! `#[no_mangle]` wasm-split exports in the crate that declares them, and any
//! client linking that crate retains the whole route code however unused; the
//! website browser entry links the library crate only, so its bundle carries
//! none of the product routes. The one shared backend still renders this
//! route tree server-side through `monochange_app`'s composition.
#![recursion_limit = "256"]
pub mod app;
pub mod pages;
mod resource_codec;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

/// Hydrate the projects app on the client (WASM only).
///
/// The projects `leptoskit.toml` selects this crate as its browser package
/// with the `hydrate` render feature, so `pkg/projects/monochange_projects_web.js`
/// loads this entry and hydrates the server-rendered product page. The
/// website client never links this crate, so each bundle carries exactly one
/// `hydrate` export. Product RPCs share the page origin with Class B Axum
/// routes via the leptoskit_rpc web client.
#[cfg(all(target_arch = "wasm32", feature = "hydrate"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
	console_error_panic_hook::set_once();
	_ = console_log::init_with_level(log::Level::Debug);
	_ = any_spawner::Executor::init_wasm_bindgen();

	// Lazy routes hydrate through their own chunk-loading mount; hydrate_body
	// panics when it meets one.
	leptos::mount::hydrate_lazy(crate::app::ProjectsApp);
}
