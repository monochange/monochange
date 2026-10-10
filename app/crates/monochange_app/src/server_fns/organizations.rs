//! Organisations and the projects created from them.
//!
//! Access always derives from the repositories a user can manage: an
//! organisation appears when at least one of its repositories is accessible,
//! and only those repositories can join its projects.

#[cfg(test)]
#[path = "__tests__/organizations_tests.rs"]
mod tests;

use leptos::server;
use serde::Deserialize;
use serde::Serialize;

use super::repos::RepoInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
	pub slug: String,
	pub name: String,
	pub description: String,
	pub repository_count: i64,
}

/// One connected account and what the user can do with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
	format!("/dashboard/{login}")
}

pub fn project_path(organization: &str, project: &str) -> String {
	format!("/dashboard/{organization}/projects/{project}")
}

pub fn feedback_console_path(organization: &str, project: &str) -> String {
	format!("{}/feedback", project_path(organization, project))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRepositoryView {
	pub full_name: String,
	/// `None` while the repository is disconnected from monochange.
	pub private: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetails {
	pub organization: String,
	pub personal: bool,
	pub project: ProjectSummary,
	pub repositories: Vec<ProjectRepositoryView>,
	/// The organisation's accessible repositories, for editing the selection.
	pub available: Vec<RepoInfo>,
}

/// An organisation the user may manage, with its accessible repositories.
#[cfg(not(target_arch = "wasm32"))]
struct OrganizationAccess {
	login: String,
	personal: bool,
	record: Option<monochange_app_db::projects::OrganizationRecord>,
	repositories: Vec<super::repos::AccessibleRepository>,
}

#[cfg(not(target_arch = "wasm32"))]
type ServerResult<T> = Result<T, server_fn::ServerFnError>;

#[cfg(not(target_arch = "wasm32"))]
fn database_error(error: impl std::fmt::Display) -> server_fn::ServerFnError {
	server_fn::ServerFnError::new(format!("DB: {error}"))
}

/// Links installations recorded before organisations existed by asking the
/// GitHub App for their account. Failures leave the installation unlinked and
/// are retried on the next visit.
#[cfg(not(target_arch = "wasm32"))]
async fn link_legacy_installations(
	state: &monochange_app_api::AppState,
	repositories: &mut [super::repos::AccessibleRepository],
) -> ServerResult<()> {
	let Some(app) = state.github_app.as_ref() else {
		return Ok(());
	};
	let mut unlinked: Vec<(i32, i64)> = repositories
		.iter()
		.filter(|repository| repository.organization_id.is_none())
		.map(|repository| {
			(
				repository.installation_id,
				repository.github_installation_id,
			)
		})
		.collect();
	unlinked.sort_unstable();
	unlinked.dedup();
	for (installation_id, github_installation_id) in unlinked {
		let account = match app
			.installation_account(&state.http, github_installation_id)
			.await
		{
			Ok(account) => account,
			Err(error) => {
				tracing::warn!(%error, installation = github_installation_id, "installation account lookup failed");
				continue;
			}
		};
		let mut connection = state.db.acquire().await.map_err(database_error)?;
		let organization_id = monochange_app_db::projects::link_installation_organization(
			&mut connection,
			installation_id,
			&account,
		)
		.await
		.map_err(database_error)?;
		for repository in repositories
			.iter_mut()
			.filter(|repository| repository.installation_id == installation_id)
		{
			repository.organization_id = Some(organization_id);
		}
	}
	Ok(())
}

/// Every organisation the signed-in user can manage, grouped from their
/// accessible repositories. Empty when signed out.
#[cfg(not(target_arch = "wasm32"))]
async fn accessible_organizations(
	state: &monochange_app_api::AppState,
) -> ServerResult<Option<(i32, Vec<OrganizationAccess>)>> {
	let Some(user_id) = super::repos::signed_in_user_id(state).await? else {
		return Ok(None);
	};
	let mut repositories = super::repos::accessible_repositories(state, user_id).await?;
	link_legacy_installations(state, &mut repositories).await?;

	let mut organizations: Vec<OrganizationAccess> = Vec::new();
	for repository in repositories {
		let login = repository.info.installation_login.clone();
		if let Some(organization) = organizations
			.iter_mut()
			.find(|organization| organization.login.eq_ignore_ascii_case(&login))
		{
			organization.repositories.push(repository);
			continue;
		}
		let record = match repository.organization_id {
			Some(_) => {
				monochange_app_db::projects::find_organization(
					&state.db,
					monochange_app_db::projects::GITHUB,
					&login,
				)
				.await
				.map_err(database_error)?
			}
			None => None,
		};
		organizations.push(OrganizationAccess {
			personal: repository.account_type == "User",
			login,
			record,
			repositories: vec![repository],
		});
	}
	organizations.sort_by_key(|organization| organization.login.to_ascii_lowercase());
	Ok(Some((user_id, organizations)))
}

#[cfg(not(target_arch = "wasm32"))]
async fn summarize(
	state: &monochange_app_api::AppState,
	organization: OrganizationAccess,
) -> ServerResult<OrganizationSummary> {
	let projects = match &organization.record {
		Some(record) => {
			monochange_app_db::projects::list_projects(&state.db, record.id)
				.await
				.map_err(database_error)?
				.into_iter()
				.map(project_summary)
				.collect()
		}
		None => Vec::new(),
	};
	Ok(OrganizationSummary {
		avatar_url: organization
			.record
			.as_ref()
			.and_then(|record| record.avatar_url.clone()),
		supports_projects: organization.record.is_some(),
		login: organization.login,
		personal: organization.personal,
		repositories: organization
			.repositories
			.into_iter()
			.map(|repository| repository.info)
			.collect(),
		projects,
	})
}

#[cfg(not(target_arch = "wasm32"))]
fn project_summary(record: monochange_app_db::projects::ProjectRecord) -> ProjectSummary {
	ProjectSummary {
		slug: record.slug,
		name: record.name,
		description: record.description,
		repository_count: record.repository_count,
	}
}

/// The organisation called `login`, if the user can manage it.
#[cfg(not(target_arch = "wasm32"))]
async fn organization_access(
	state: &monochange_app_api::AppState,
	login: &str,
) -> ServerResult<Option<(i32, OrganizationAccess)>> {
	let Some((user_id, organizations)) = accessible_organizations(state).await? else {
		return Ok(None);
	};
	Ok(organizations
		.into_iter()
		.find(|organization| organization.login.eq_ignore_ascii_case(login))
		.map(|organization| (user_id, organization)))
}

/// Turns validated full names into project repositories, refusing any the
/// user cannot manage in this organisation.
#[cfg(not(target_arch = "wasm32"))]
fn selected_repositories(
	organization: &OrganizationAccess,
	full_names: &[String],
) -> ServerResult<Vec<monochange_app_db::projects::ProjectRepositoryRecord>> {
	full_names
		.iter()
		.map(|full_name| {
			organization
				.repositories
				.iter()
				.find(|repository| {
					repository
						.info
						.github_full_name
						.eq_ignore_ascii_case(full_name)
				})
				.map(|repository| {
					monochange_app_db::projects::ProjectRepositoryRecord {
						provider: monochange_app_db::projects::GITHUB.to_owned(),
						external_id: repository.github_repo_id,
						full_name: repository.info.github_full_name.clone(),
					}
				})
				.ok_or_else(|| {
					server_fn::ServerFnError::new(format!(
						"{full_name} isn't a repository you can add to this project."
					))
				})
		})
		.collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn store_error(error: monochange_app_db::projects::ProjectStoreError) -> server_fn::ServerFnError {
	match error {
		monochange_app_db::projects::ProjectStoreError::SlugTaken(slug) => {
			server_fn::ServerFnError::new(format!(
				"A project at /{slug} already exists. Choose a different name."
			))
		}
		monochange_app_db::projects::ProjectStoreError::Database(error) => database_error(error),
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn require_projects(
	organization: &OrganizationAccess,
) -> ServerResult<&monochange_app_db::projects::OrganizationRecord> {
	organization.record.as_ref().ok_or_else(|| {
		server_fn::ServerFnError::new(
			"monochange is still confirming this account with GitHub. Reload the page and try again.",
		)
	})
}

#[cfg(not(target_arch = "wasm32"))]
fn not_found() -> server_fn::ServerFnError {
	server_fn::ServerFnError::new("That organisation or project isn't available to you.")
}

/// A project the signed-in user maintains, resolved for feedback.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct MaintainedProject {
	pub organization: String,
	pub project: ProjectSummary,
	pub maintainer: String,
	pub scope: monochange_app_api::feedback::ProjectScope,
}

/// Resolves `organization/project` for its maintainer, or `None` when it
/// isn't theirs. Only connected repositories join the scope: feedback can't
/// act on a repository monochange can no longer reach.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn maintained_project(
	state: &monochange_app_api::AppState,
	organization: &str,
	project: &str,
) -> ServerResult<Option<MaintainedProject>> {
	let Some((user_id, access)) = organization_access(state, organization).await? else {
		return Ok(None);
	};
	let Some(record) = access.record.as_ref() else {
		return Ok(None);
	};
	let Some(found) = monochange_app_db::projects::find_project(&state.db, record.id, project)
		.await
		.map_err(database_error)?
	else {
		return Ok(None);
	};
	let linked = monochange_app_db::projects::project_repositories(&state.db, found.id)
		.await
		.map_err(database_error)?;
	let repositories: Vec<_> = access
		.repositories
		.iter()
		.filter(|repository| {
			linked
				.iter()
				.any(|linked| linked.external_id == repository.github_repo_id)
		})
		.map(|repository| {
			monochange_app_api::feedback::ProjectRepository {
				full_name: repository.info.github_full_name.clone(),
				private: repository.info.github_private,
				github_installation_id: repository.github_installation_id,
			}
		})
		.collect();
	let maintainer: String = sqlx::query_scalar("SELECT github_login FROM users WHERE id = $1")
		.bind(user_id)
		.fetch_one(&state.db)
		.await
		.map_err(database_error)?;
	Ok(Some(MaintainedProject {
		organization: access.login.clone(),
		scope: monochange_app_api::feedback::ProjectScope {
			project_id: found.id,
			disconnected: linked.len().saturating_sub(repositories.len()),
			repositories,
		},
		project: project_summary(found),
		maintainer,
	}))
}

/// The signed-in user's organisations with their repositories and projects.
#[server]
pub async fn list_organizations() -> Result<Vec<OrganizationSummary>, server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let Some((_, organizations)) = accessible_organizations(&state).await? else {
		return Ok(Vec::new());
	};
	let mut summaries = Vec::with_capacity(organizations.len());
	for organization in organizations {
		summaries.push(summarize(&state, organization).await?);
	}
	Ok(summaries)
}

/// One organisation, or `None` when it isn't the user's to manage.
#[server]
pub async fn organization_overview(
	organization: String,
) -> Result<Option<OrganizationSummary>, server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	match organization_access(&state, &organization).await? {
		Some((_, access)) => Ok(Some(summarize(&state, access).await?)),
		None => Ok(None),
	}
}

/// Creates a project from some of an organisation's repositories, then
/// opens it.
#[server]
pub async fn create_project(
	organization: String,
	name: String,
	description: String,
	#[server(default)] repositories: Vec<String>,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let input = crate::projects::validate_project(&name, &description, repositories)
		.map_err(|error| server_fn::ServerFnError::new(error.to_string()))?;
	let (user_id, access) = organization_access(&state, &organization)
		.await?
		.ok_or_else(not_found)?;
	let record = require_projects(&access)?;
	let draft = monochange_app_db::projects::ProjectDraft {
		repositories: selected_repositories(&access, &input.repositories)?,
		slug: input.slug,
		name: input.name,
		description: input.description,
	};
	monochange_app_db::projects::create_project(&state.db, record.id, user_id, &draft)
		.await
		.map_err(store_error)?;
	leptos_axum::redirect(&project_path(&access.login, &draft.slug));
	Ok(())
}

/// A project with its repositories, or `None` when it isn't the user's.
#[server]
pub async fn project_overview(
	organization: String,
	project: String,
) -> Result<Option<ProjectDetails>, server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let Some((_, access)) = organization_access(&state, &organization).await? else {
		return Ok(None);
	};
	let Some(record) = access.record.as_ref() else {
		return Ok(None);
	};
	let Some(found) = monochange_app_db::projects::find_project(&state.db, record.id, &project)
		.await
		.map_err(database_error)?
	else {
		return Ok(None);
	};
	let repositories = monochange_app_db::projects::project_repositories(&state.db, found.id)
		.await
		.map_err(database_error)?
		.into_iter()
		.map(|repository| {
			let connected = access
				.repositories
				.iter()
				.find(|accessible| accessible.github_repo_id == repository.external_id);
			ProjectRepositoryView {
				full_name: connected.map_or(repository.full_name, |accessible| {
					accessible.info.github_full_name.clone()
				}),
				private: connected.map(|accessible| accessible.info.github_private),
			}
		})
		.collect();
	Ok(Some(ProjectDetails {
		organization: access.login.clone(),
		personal: access.personal,
		project: project_summary(found),
		repositories,
		available: access
			.repositories
			.into_iter()
			.map(|repository| repository.info)
			.collect(),
	}))
}

/// Renames a project or changes its repositories.
#[server]
pub async fn update_project(
	organization: String,
	project: String,
	name: String,
	description: String,
	#[server(default)] repositories: Vec<String>,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let input = crate::projects::validate_project(&name, &description, repositories)
		.map_err(|error| server_fn::ServerFnError::new(error.to_string()))?;
	let (_, access) = organization_access(&state, &organization)
		.await?
		.ok_or_else(not_found)?;
	let record = require_projects(&access)?;
	let existing = monochange_app_db::projects::find_project(&state.db, record.id, &project)
		.await
		.map_err(database_error)?
		.ok_or_else(not_found)?;
	let draft = monochange_app_db::projects::ProjectDraft {
		repositories: selected_repositories(&access, &input.repositories)?,
		slug: input.slug,
		name: input.name,
		description: input.description,
	};
	monochange_app_db::projects::update_project(&state.db, existing.id, &draft)
		.await
		.map_err(store_error)?;
	leptos_axum::redirect(&project_path(&access.login, &draft.slug));
	Ok(())
}

/// Deletes a project. Its repositories stay connected to monochange.
#[server]
pub async fn delete_project(
	organization: String,
	project: String,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let (_, access) = organization_access(&state, &organization)
		.await?
		.ok_or_else(not_found)?;
	let record = require_projects(&access)?;
	let existing = monochange_app_db::projects::find_project(&state.db, record.id, &project)
		.await
		.map_err(database_error)?
		.ok_or_else(not_found)?;
	monochange_app_db::projects::delete_project(&state.db, existing.id)
		.await
		.map_err(database_error)?;
	leptos_axum::redirect(&organization_path(&access.login));
	Ok(())
}
