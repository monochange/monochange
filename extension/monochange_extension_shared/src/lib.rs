//! Extension-specific presentation helpers over the shared product contracts.
pub use monochange_shared::OrganizationSummary;
pub use monochange_shared::ProjectSummary;
pub use monochange_shared::RepoInfo;
pub use monochange_shared::SessionUser;
pub use monochange_shared::WebsiteEntry;
pub use monochange_shared::WebsiteRelease;
pub use monochange_shared::WebsiteSection;
/// Compact status line for the popup header.
#[must_use]
pub fn release_status_label(releases: &[WebsiteRelease]) -> String {
	match releases.first() {
		Some(latest) => format!("v{} on monochange.dev", latest.version),
		None => "No published website releases".to_owned(),
	}
}

/// Counts projects across every organisation for the glance UI.
#[must_use]
pub fn project_count(organizations: &[OrganizationSummary]) -> usize {
	organizations
		.iter()
		.map(|organization| organization.projects.len())
		.sum()
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
