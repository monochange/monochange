//! The projects browser entry: product routes, navigation, and hydration.
//!
//! The projects frontend owns the product root (`/projects`), the public
//! `/p/:organization/:project` feedback portals, and their hydration. The
//! website backend composes this component into SSR responses for product
//! paths; the projects build hydrates it independently, loading only its own
//! entry bundle from `pkg/projects/`.

use leptos::prelude::*;
use leptos_meta::Stylesheet;
use leptos_meta::Title;
use leptos_meta::provide_meta_context;
use leptos_router::Lazy;
use leptos_router::components::Route;
use leptos_router::components::Router;
use leptos_router::components::Routes;
use leptos_router::path;

use crate::error::ErrorTemplate;
use crate::pages::feedback_console::FeedbackConsoleRoute;
use crate::pages::home::ProjectsNavigation;
use crate::pages::organization::OrganizationRoute;
use crate::pages::portal::PortalRoute;
use crate::pages::project::ProjectRoute;

/// Root `<ProjectsApp/>` component: the product browser entry.
#[component]
pub fn ProjectsApp() -> impl IntoView {
	provide_meta_context();

	view! {
		<Stylesheet id="leptos" href=monochange_shared::routing::PROJECTS_STYLESHEET />
		// Same default document title as the combined app before the split;
		// individual pages override it through the meta context.
		<Title text="monochange — Release planning for monorepos" />
		<Router>
			<ProjectsNavigation />
			<main id="main-content" class="min-h-screen">
				<Routes fallback=|| {
					view! { <ErrorTemplate status=404 message="Page not found" /> }
				}>
					<Route path=path!("/projects") view={Lazy::<OrganizationRoute>::new()} />
					<Route
						path=path!("/projects/:organization")
						view={Lazy::<OrganizationRoute>::new()}
					/>
					<Route
						path=path!("/projects/:organization/projects/:project")
						view={Lazy::<ProjectRoute>::new()}
					/>
					<Route
						path=path!("/projects/:organization/projects/:project/feedback")
						view={Lazy::<FeedbackConsoleRoute>::new()}
					/>
					<Route
						path=path!("/p/:organization/:project")
						view={Lazy::<PortalRoute>::new()}
					/>
				</Routes>
			</main>
		</Router>
	}
}

#[cfg(test)]
#[path = "__tests__/app_tests.rs"]
mod tests;

/// Hydrate the projects app on the client (WASM only).
///
/// The projects `leptoskit.toml` selects this crate as its browser package
/// and enables `browser_entry`, so `pkg/projects/monochange_projects.js`
/// loads this entry and hydrates the server-rendered product page. The
/// website client links this crate without `browser_entry`, so its wasm
/// carries only one `hydrate` export. Product RPCs share the page origin
/// with Class B Axum routes via the leptoskit_rpc web client.
#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
	console_error_panic_hook::set_once();
	_ = console_log::init_with_level(log::Level::Debug);
	_ = any_spawner::Executor::init_wasm_bindgen();

	// Lazy routes hydrate through their own chunk-loading mount; hydrate_body
	// panics when it meets one.
	leptos::mount::hydrate_lazy(crate::app::ProjectsApp);
}
