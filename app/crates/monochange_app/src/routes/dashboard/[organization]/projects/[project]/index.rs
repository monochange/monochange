//! `/dashboard/:organization/projects/:project`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// One project.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct ProjectPage {
	pub organization: String,
	pub project: String,
}

pub fn page(route: &ProjectPage) -> Element {
	crate::pages::project::project_screen(route.organization.clone(), route.project.clone())
}
