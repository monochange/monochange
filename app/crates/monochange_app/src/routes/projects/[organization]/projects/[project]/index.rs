//! `/projects/:organization/projects/:project`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// One project inside the projects shell.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct ProjectDetailsPage {
	pub organization: String,
	pub project: String,
}

pub fn page(route: &ProjectDetailsPage) -> Element {
	crate::pages::project::project_screen(route.organization.clone(), route.project.clone())
}
