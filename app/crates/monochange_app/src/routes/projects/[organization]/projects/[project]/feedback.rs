//! `/projects/:organization/projects/:project/feedback`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// Feedback console inside the projects shell.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct ProjectFeedback {
	pub organization: String,
	pub project: String,
}

pub fn page(route: &ProjectFeedback) -> Element {
	crate::pages::feedback_console::console_screen(
		route.organization.clone(),
		route.project.clone(),
	)
}
