//! Which browser entry owns a path, and when a link must leave it.
//!
//! The site serves two independently built browser frontends behind one
//! origin: the website entry (landing, auth, docs) and the projects entry
//! (`/projects`, `/p` feedback portals). The leptos router intercepts
//! same-origin anchor clicks for client-side navigation, so a link whose
//! destination belongs to the other entry must be a real document
//! navigation — otherwise the loaded bundle would render a 404 fallback
//! instead of the other frontend's page.

/// The browser frontends served behind the one origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
	/// The website entry: landing, auth, docs, and the repositories dashboard.
	Website,
	/// The projects entry: `/projects` routes and `/p` feedback portals.
	Projects,
}

/// The pkg directory, under the site root, holding the website entry's bundle.
///
/// The leptoskit build pipeline places each frontend's pkg output under its
/// namespace (`[[plugins]]` options in `app/leptoskit.toml` and
/// `app/projects/leptoskit.toml`); these constants mirror that layout for the
/// server-side hydration scripts and the static-file handler.
pub const WEBSITE_PKG_URL: &str = "pkg/site";
/// The pkg directory holding the projects entry's bundle.
pub const PROJECTS_PKG_URL: &str = "pkg/projects";

/// The website browser entry's file stem, matching the client lib name the
/// website `leptoskit.toml` selects.
pub const WEBSITE_OUTPUT_NAME: &str = "monochange_app";
/// The projects browser entry's file stem, matching the client lib name the
/// projects `leptoskit.toml` selects.
pub const PROJECTS_OUTPUT_NAME: &str = "monochange_projects";

/// The website entry's stylesheet URL.
pub const WEBSITE_STYLESHEET: &str = "/pkg/site/monochange_app.css";
/// The projects entry's stylesheet URL.
pub const PROJECTS_STYLESHEET: &str = "/pkg/projects/monochange_projects.css";

/// The website entry's hydration module URL.
pub const WEBSITE_ENTRY_JS: &str = "/pkg/site/monochange_app.js";
/// The website entry's wasm module URL.
pub const WEBSITE_ENTRY_WASM: &str = "/pkg/site/monochange_app_bg.wasm";
/// The projects entry's hydration module URL.
pub const PROJECTS_ENTRY_JS: &str = "/pkg/projects/monochange_projects.js";
/// The projects entry's wasm module URL.
pub const PROJECTS_ENTRY_WASM: &str = "/pkg/projects/monochange_projects_bg.wasm";

/// The wasm-split loader file name. The name is fixed: leptos'
/// `HydrationScripts` and the leptoskit build pipeline both look for it.
pub const SPLIT_LOADER: &str = "__wasm_split.______________________.js";
/// The wasm-split manifest file name each frontend's pkg namespace holds.
pub const SPLIT_MANIFEST: &str = "__wasm_split_manifest.json";

/// Whether the projects browser entry owns `path`.
///
/// Both callers must pass a pathname (no origin, query allowed but ignored):
/// SSR reads it from the request, the browser from `window.location`. The
/// projects surface is prefix-based, so this must stay in sync with the
/// routes its router declares.
#[must_use]
pub fn is_projects_path(path: &str) -> bool {
	let path = path.split(['?', '#']).next().unwrap_or(path);
	path == "/projects" || path.starts_with("/projects/") || path == "/p" || path.starts_with("/p/")
}

/// The browser entry that owns `path`.
#[must_use]
pub fn surface_for_path(path: &str) -> Surface {
	if is_projects_path(path) {
		Surface::Projects
	} else {
		Surface::Website
	}
}

/// The anchor `target` that forces a document navigation when `href` leaves
/// the browser entry that owns `current`, or `None` for a same-surface link.
///
/// `target="_self"` is the browser default, so the attribute changes nothing
/// for the router's anchor interception except opting the link out of it:
/// the leptos router skips anchors with any explicit target. Cross-origin
/// hrefs are never intercepted anyway and return `None`.
#[must_use]
pub fn cross_surface_target(current: &str, href: &str) -> Option<&'static str> {
	let href = href.split(['?', '#']).next().unwrap_or(href);

	if !href.starts_with('/') {
		return None;
	}

	(surface_for_path(current) != surface_for_path(href)).then_some("_self")
}

#[cfg(test)]
#[path = "__tests__/routing_tests.rs"]
mod tests;
