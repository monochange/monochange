//! Canonical URLs shared by both browser entries.

use leptos::prelude::*;
use leptos_meta::Link;
use leptos_router::hooks::use_location;

/// Publish the current path without tracking parameters as its canonical URL.
#[component]
pub fn Canonical() -> impl IntoView {
	let location = use_location();
	view! {
		{move || {
			let href = format!("https://monochange.dev{}", location.pathname.get());
			view! { <Link rel="canonical" href=href /> }
		}}
	}
}
