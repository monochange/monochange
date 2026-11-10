//! One resource per request, shared by the layout and the page that needs it.
//!
//! File-router `page` functions run on every render. Creating a `Resource`
//! there would refetch on each pass, so the first call stores it on the
//! current owner.

use leptos::prelude::*;

use crate::server_fns::auth::SessionUser;
use crate::server_fns::auth::get_session;

/// The signed-in user, loaded once for the document.
pub fn session() -> Resource<Result<Option<SessionUser>, server_fn::ServerFnError>> {
	once(|| Resource::new_blocking(|| (), |()| get_session()))
}

/// Returns `init()` the first time `T` is requested on this owner.
pub fn once<T>(init: impl FnOnce() -> T) -> T
where
	T: Copy + Send + Sync + 'static,
{
	let slot = use_context::<RwSignal<Option<T>>>().unwrap_or_else(|| {
		let created = RwSignal::new(None);
		provide_context(created);
		created
	});

	if let Some(value) = slot.get_untracked() {
		return value;
	}

	let value = init();
	slot.set(Some(value));
	value
}
