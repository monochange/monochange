//! The signed-in and signed-out workspace landing page.

use leptos::prelude::*;
use leptos_router::LazyRoute;
use leptos_router::lazy_route;

/// Workspace landing page, loaded separately from organization details.
pub struct ProjectsHomeRoute;

#[lazy_route]
impl LazyRoute for ProjectsHomeRoute {
	fn data() -> Self {
		Self
	}

	fn view(_this: Self) -> AnyView {
		view! { <monochange_projects::pages::home::ProjectsHome /> }.into_any()
	}
}
