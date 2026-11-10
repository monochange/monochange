//! `/dashboard/:organization/projects/:project/feedback`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// The maintainer feedback console.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct FeedbackConsolePage {
	pub organization: String,
	pub project: String,
}

pub fn page(route: &FeedbackConsolePage) -> Element {
	crate::pages::feedback_console::console_screen(
		route.organization.clone(),
		route.project.clone(),
	)
}

// The file router rejects a `__tests__` directory inside `routes/`.
#[cfg(test)]
#[path = "../../../../../__tests__/routes/dashboard_feedback_tests.rs"]
mod tests;
