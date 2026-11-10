//! `/p/:organization/:project`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// The public feedback portal.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct PortalPage {
	pub organization: String,
	pub project: String,
	/// Set after a no-JavaScript share redirects back with the new item id.
	pub shared: Option<String>,
}

pub fn page(route: &PortalPage) -> Element {
	crate::pages::portal::portal_screen(
		route.organization.clone(),
		route.project.clone(),
		route.shared.clone(),
	)
}
