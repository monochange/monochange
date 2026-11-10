//! `/projects/:organization`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// One account inside the projects shell.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct ProjectOrganization {
	pub organization: String,
	pub q: Option<String>,
}

pub fn page(route: &ProjectOrganization) -> Element {
	crate::pages::organization::organization_screen(route.organization.clone(), route.q.clone())
}
