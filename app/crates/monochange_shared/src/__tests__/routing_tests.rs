//! Path-to-surface classification shared by the website and projects entries.

use rstest::rstest;

use super::Surface;
use super::cross_surface_target;
use super::is_projects_path;
use super::surface_for_path;

#[rstest]
#[case("/projects", true)]
#[case("/projects/", true)]
#[case("/projects/acme", true)]
#[case("/projects/acme/projects/web/feedback", true)]
#[case("/p", true)]
#[case("/p/acme/web", true)]
#[case("/p/acme/web#share", true)]
#[case("/p/acme/web?next=/projects", true)]
#[case("/", false)]
#[case("/pricing", false)]
#[case("/projects-x", false)]
#[case("/portal", false)]
#[case("/dashboard", false)]
#[case("/dashboard/acme", false)]
#[case("/login", false)]
#[case("/login?next=%2Fprojects", false)]
#[case("", false)]
fn paths_classify_by_owner(#[case] path: &str, #[case] projects: bool) {
	assert_eq!(is_projects_path(path), projects, "{path}");
}

#[rstest]
#[case("/p/acme/web", Surface::Projects)]
#[case("/projects/acme", Surface::Projects)]
#[case("/", Surface::Website)]
#[case("/changelog", Surface::Website)]
fn surfaces_match_paths(#[case] path: &str, #[case] expected: Surface) {
	assert_eq!(surface_for_path(path), expected);
}

#[rstest]
// Navigating from the website into product routes reloads the document.
#[case("/", "/projects", Some("_self"))]
#[case("/login", "/projects", Some("_self"))]
#[case("/dashboard", "/projects/acme", Some("_self"))]
#[case("/", "/login?next=%2Fprojects", None)]
// Navigating from projects back to website surfaces reloads the document.
#[case("/projects", "/dashboard", Some("_self"))]
#[case("/projects/acme", "/account", Some("_self"))]
#[case("/p/acme/web", "/login?next=%2Fp%2Facme%2Fweb", Some("_self"))]
// Links inside one entry stay client-side.
#[case("/projects", "/projects/acme", None)]
#[case("/projects/acme", "/projects", None)]
#[case("/p/acme/web", "/p/acme/other", None)]
#[case("/", "/install", None)]
#[case("/", "/pricing#demo", None)]
// External and non-path hrefs are never intercepted in the first place.
#[case("/", "https://github.com/monochange/monochange", None)]
#[case("/projects", "#share", None)]
#[case("/", "", None)]
fn cross_surface_links_need_document_navigation(
	#[case] current: &str,
	#[case] href: &str,
	#[case] target: Option<&'static str>,
) {
	assert_eq!(
		cross_surface_target(current, href),
		target,
		"{current} -> {href}"
	);
}
