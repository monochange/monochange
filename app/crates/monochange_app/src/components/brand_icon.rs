//! Small, locally served ecosystem marks.

use leptos::prelude::*;

/// Decorative icon; the adjacent text supplies its accessible name.
#[component]
pub fn BrandIcon(name: &'static str) -> impl IntoView {
	view! { <span class="brand-icon" aria-hidden="true" style=format!("--brand-icon: url('/icons/{name}.svg')") /> }
}
