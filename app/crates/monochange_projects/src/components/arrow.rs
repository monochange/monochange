//! Shared directional icon for navigation actions.

use leptos::prelude::*;

/// Decorative arrow; the surrounding action supplies its accessible name.
/// Style it through its parent; its SVG markup is inert.
#[component]
pub fn ArrowIcon() -> impl IntoView {
	// Fixed decorative markup needs no attribute-by-attribute hydration.
	leptos::svg::InertElement::new(
		r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 12h16m-6-6 6 6-6 6"></path></svg>"#,
	)
}
