//! Error page for the projects browser entry.
//!
//! One implementation serves both browser frontends: the website re-exports
//! this component, so unknown paths on either surface render the same error
//! page they always have.

use leptos::prelude::*;
use monochange_shared::routing::cross_surface_target;

/// The pathname this error page renders at, so links can classify whether
/// they leave the browser entry that owns the page. Error pages always
/// render inside a router: route fallbacks and the book pages both mount one.
fn current_path() -> String {
	leptos_router::hooks::use_location()
		.pathname
		.get_untracked()
}

/// The message to show for a failed server action.
pub fn action_error(error: &server_fn::ServerFnError) -> String {
	match error {
		server_fn::ServerFnError::ServerError(message) => message.clone(),
		other => other.to_string(),
	}
}

/// Error page component.
#[component]
pub fn ErrorTemplate(
	#[prop(default = 500)] status: u16,
	#[prop(default = "Something went wrong.")] message: &'static str,
) -> impl IntoView {
	let title = move || {
		match status {
			404 => "Page not found",
			401 => "Not authorized",
			_ => "Server error",
		}
	};

	view! {
		<div class="site-width status-page">
			<div class="text-center">
				<img src="/branding/mark.svg" width="64" height="64" alt="" class="mx-auto" />
				<h1>
					{status}
				</h1>
				<h2>
					{title}
				</h2>
				<p>{message}</p>
				<div class="actions">
					// "Go home" crosses the website/projects boundary when the
					// error page renders on a projects path, so it must load
					// the website entry as a document navigation.
					<a href="/" target=move || cross_surface_target(&current_path(), "/") class="button button-brand">
						<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
							<path stroke-linecap="round" stroke-linejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
						</svg>
						Go home
					</a>
				</div>
			</div>
		</div>
	}
}

#[cfg(test)]
#[path = "__tests__/error_tests.rs"]
mod tests;
