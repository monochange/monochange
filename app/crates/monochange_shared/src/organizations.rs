//! Organisation and project wire types for the signed-in workspace.

use serde::Deserialize;
use serde::Serialize;

use crate::repos::RepoInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectSummary {
	pub slug: String,
	pub name: String,
	pub description: String,
	pub repository_count: i64,
}

/// One connected account and what the user can do with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OrganizationSummary {
	pub login: String,
	/// A personal account rather than a GitHub organisation.
	pub personal: bool,
	pub avatar_url: Option<String>,
	/// Projects need the account's stable id. Installations recorded before
	/// organisations existed get it from GitHub on first visit; until then
	/// this is `false`.
	pub supports_projects: bool,
	pub repositories: Vec<RepoInfo>,
	pub projects: Vec<ProjectSummary>,
}

impl OrganizationSummary {
	pub fn path(&self) -> String {
		organization_path(&self.login)
	}
}

pub fn organization_path(login: &str) -> String {
	format!("/projects/{login}")
}

pub fn project_path(organization: &str, project: &str) -> String {
	format!("/projects/{organization}/projects/{project}")
}

pub fn feedback_console_path(organization: &str, project: &str) -> String {
	format!("{}/feedback", project_path(organization, project))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectRepositoryView {
	pub full_name: String,
	/// `None` while the repository is disconnected from monochange.
	pub private: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectDetails {
	pub organization: String,
	pub personal: bool,
	pub project: ProjectSummary,
	pub repositories: Vec<ProjectRepositoryView>,
	/// The organisation's accessible repositories, for editing the selection.
	pub available: Vec<RepoInfo>,
}
