//! `/dashboard/:organization`

use leptoskit::prelude::*;
use leptoskit_router::Route;

/// One account's projects and repositories.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct OrganizationPage {
	pub organization: String,
	pub q: Option<String>,
}

pub fn page(route: &OrganizationPage) -> Element {
	crate::pages::organization::organization_screen(route.organization.clone(), route.q.clone())
}
