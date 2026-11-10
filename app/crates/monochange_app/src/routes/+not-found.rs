//! Unmatched URLs.

use leptoskit::prelude::*;

use crate::ui::heading;
use crate::ui::link;
use crate::ui::text;

pub fn page(route: &leptoskit_router::NotFound) -> Element {
	mark_not_found();
	let title = heading(1, "Page not found");
	let detail = text(format!(
		"/{} isn't part of monochange.",
		route.path.join("/")
	));
	let home = link("/", "Back to monochange");
	view! {
		<View class="flex flex-col gap-4">
			{title}
			{detail}
			{home}
		</View>
	}
}

fn mark_not_found() {
	#[cfg(not(target_arch = "wasm32"))]
	if let Some(options) = leptos::prelude::use_context::<leptos_axum::ResponseOptions>() {
		options.set_status(axum::http::StatusCode::NOT_FOUND);
	}
}
