#![allow(unsafe_code)] // Native guest entry points are exported C symbols.

//! monochange native guest: Android / iOS screens over leptoskit.
pub mod screens;
pub mod session;
pub mod tasks;
pub mod theme;
pub mod ui;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

/// Server origin for native HTTP plugin calls. Override at build time with
/// `MONOCHANGE_SERVER`.
pub const SERVER: &str = match option_env!("MONOCHANGE_SERVER") {
	Some(server) => server,
	None => "https://monochange.dev",
};

/// Mounts the shared Element tree in the native Wasm runtime.
#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
#[unsafe(no_mangle)]
pub extern "C" fn leptoskit_main() {
	let owner = reactive_graph::owner::Owner::new();
	let model = owner.with(screens::home::HomeModel::restore);
	leptoskit::guest::mount(move || {
		owner.with(|| leptoskit::view! { <screens::home::Home model=model /> })
	});
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
#[unsafe(no_mangle)]
pub extern "C" fn leptoskit_event(node: i64, kind: i32, ptr: i32, len: i32) -> i32 {
	let handled = leptoskit::guest::dispatch(node, kind, ptr, len);
	tasks::poll();
	handled
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
#[unsafe(no_mangle)]
pub extern "C" fn leptoskit_poll() {
	tasks::poll();
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
#[unsafe(no_mangle)]
pub extern "C" fn leptoskit_alloc(len: i32) -> i32 {
	leptoskit::guest::alloc(len)
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
#[unsafe(no_mangle)]
pub extern "C" fn leptoskit_abi_version() -> i32 {
	leptoskit::guest::ABI_VERSION as i32
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
#[unsafe(no_mangle)]
pub extern "C" fn leptoskit_flush() {
	leptoskit::guest::flush();
}
