//! Use the runtime's executor so task completion settles reactive regions
//! without rebuilding the application while its initial view is mounting.
use std::future::Future;

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub use leptoskit::guest::poll;

/// Queue work on the executor owned by the current UI runtime.
pub fn spawn(task: impl Future<Output = ()> + 'static) {
	#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
	leptoskit::guest::spawn(task);
	#[cfg(all(target_arch = "wasm32", feature = "web"))]
	leptos::task::spawn_local(task);
	#[cfg(not(target_arch = "wasm32"))]
	let _ = task;
}
