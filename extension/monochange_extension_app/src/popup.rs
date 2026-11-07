//! Popup glance model: brand chrome, session, projects, release status.

use monochange_extension_shared::OrganizationSummary;
use monochange_extension_shared::SessionUser;
use monochange_extension_shared::WebsiteRelease;
use monochange_extension_shared::project_count;
use monochange_extension_shared::release_status_label;

/// What the popup renders for one load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupState {
	/// Still talking to the backend.
	Loading,
	/// Signed out — offer GitHub sign-in.
	SignedOut { login_url: String },
	/// Signed in with projects and/or release status.
	Ready(PopupReady),
	/// Transport or decode failure.
	Error { message: String },
}

/// Signed-in popup contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupReady {
	pub user: SessionUser,
	pub organizations: Vec<OrganizationSummary>,
	pub releases: Vec<WebsiteRelease>,
}

impl PopupReady {
	/// Short status under the brand lockup.
	#[must_use]
	pub fn status_line(&self) -> String {
		release_status_label(&self.releases)
	}

	/// Total projects across organisations.
	#[must_use]
	pub fn projects(&self) -> usize {
		project_count(&self.organizations)
	}

	/// Flattened project rows for the glance list (org / name).
	#[must_use]
	pub fn project_rows(&self) -> Vec<ProjectRow> {
		let mut rows = Vec::new();
		for organization in &self.organizations {
			for project in &organization.projects {
				rows.push(ProjectRow {
					organization: organization.login.clone(),
					slug: project.slug.clone(),
					name: project.name.clone(),
					repository_count: project.repository_count,
				});
			}
		}
		rows
	}
}

/// One project line in the popup list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRow {
	pub organization: String,
	pub slug: String,
	pub name: String,
	pub repository_count: i64,
}

impl ProjectRow {
	/// Secondary text for the row.
	#[must_use]
	pub fn meta(&self) -> String {
		let repos = self.repository_count;
		let noun = if repos == 1 { "repo" } else { "repos" };
		format!("{}/{} · {repos} {noun}", self.organization, self.slug)
	}
}

/// Messages the popup sends the background worker.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExtensionMessage {
	/// Reload session + glance data.
	Refresh,
	/// Forget the local bearer after server logout.
	SignedOut,
}

/// Worker replies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExtensionReply {
	State(PopupWireState),
	Done,
	Failed(String),
}

/// Serde-friendly popup state for worker ↔ surface messages.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PopupWireState {
	Loading,
	SignedOut {
		login_url: String,
	},
	Ready {
		user: SessionUser,
		organizations: Vec<OrganizationSummary>,
		releases: Vec<WebsiteRelease>,
	},
	Error {
		message: String,
	},
}

impl From<PopupState> for PopupWireState {
	fn from(value: PopupState) -> Self {
		match value {
			PopupState::Loading => Self::Loading,
			PopupState::SignedOut { login_url } => Self::SignedOut { login_url },
			PopupState::Ready(ready) => {
				Self::Ready {
					user: ready.user,
					organizations: ready.organizations,
					releases: ready.releases,
				}
			}
			PopupState::Error { message } => Self::Error { message },
		}
	}
}

impl From<PopupWireState> for PopupState {
	fn from(value: PopupWireState) -> Self {
		match value {
			PopupWireState::Loading => Self::Loading,
			PopupWireState::SignedOut { login_url } => Self::SignedOut { login_url },
			PopupWireState::Ready {
				user,
				organizations,
				releases,
			} => {
				Self::Ready(PopupReady {
					user,
					organizations,
					releases,
				})
			}
			PopupWireState::Error { message } => Self::Error { message },
		}
	}
}
