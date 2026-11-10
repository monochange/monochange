//! `/dashboard`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// The workspace, filtered by `q` and `org`.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct Workspace {
	pub q: Option<String>,
	pub org: Option<String>,
}

pub fn page(route: &Workspace) -> Element {
	crate::pages::dashboard::dashboard_screen(route.q.clone(), route.org.clone())
}
