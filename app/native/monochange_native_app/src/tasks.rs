//! Running async work: each host has its own executor.
//!
//! A native guest keeps a local pool driven by `leptoskit_poll` after plugin
//! replies. Tests on the host use Tokio's local set.

use std::future::Future;

/// Starts `task` on this host's executor.
pub fn spawn(task: impl Future<Output = ()> + 'static) {
	imp::spawn(task);
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
mod imp {
	use std::cell::RefCell;

	use futures_executor::LocalPool;
	use futures_executor::LocalSpawner;
	use futures_util::task::LocalSpawnExt;

	thread_local! {
		static POOL: RefCell<LocalPool> = RefCell::new(LocalPool::new());
		static SPAWNER: LocalSpawner = POOL.with_borrow(LocalPool::spawner);
	}

	pub fn spawn(task: impl std::future::Future<Output = ()> + 'static) {
		SPAWNER
			.with(|spawner| spawner.spawn_local(task))
			.expect("the guest's pool is alive");
		poll();
	}

	pub fn poll() {
		let ran = POOL.try_with(|pool| {
			if let Ok(mut pool) = pool.try_borrow_mut() {
				pool.run_until_stalled();
				true
			} else {
				false
			}
		});

		if ran == Ok(true) {
			leptoskit::guest::render();
		}
	}
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub use imp::poll;

#[cfg(not(all(target_arch = "wasm32", feature = "native", not(feature = "web"))))]
mod imp {
	pub fn spawn(task: impl std::future::Future<Output = ()> + 'static) {
		let _ = task;
		#[cfg(not(target_arch = "wasm32"))]
		{
			// Host-side unit tests can replace this with a LocalSet when needed.
		}
	}
}
