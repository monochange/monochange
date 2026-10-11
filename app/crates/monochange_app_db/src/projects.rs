//! Organisations and the projects created from them.
//!
//! An organisation is a provider account the monochange app is installed on:
//! a GitHub organisation, or a personal account. A project groups one or more
//! of an organisation's repositories under one name. Projects reference
//! repositories by the provider's stable id, so they survive the app being
//! uninstalled and reinstalled.

use sqlx::Row;
use thiserror::Error;

use crate::DbPool;

/// The provider every row created today belongs to.
pub const GITHUB: &str = "github";

#[derive(Debug, Error)]
pub enum ProjectStoreError {
	#[error("a project with the slug `{0}` already exists in this organisation")]
	SlugTaken(String),
	#[error(transparent)]
	Database(#[from] sqlx::Error),
}

/// A provider account as reported by the provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdentity {
	pub provider: String,
	pub external_id: i64,
	pub login: String,
	/// `Organization` or `User`, as the provider names them.
	pub account_type: String,
	pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationRecord {
	pub id: i32,
	pub provider: String,
	pub external_id: i64,
	pub login: String,
	pub account_type: String,
	pub avatar_url: Option<String>,
}

/// A repository a project includes, by its provider identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRepositoryRecord {
	pub provider: String,
	pub external_id: i64,
	/// Last known `owner/name`; kept so a disconnected repository still has a
	/// readable name.
	pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
	pub id: i32,
	pub organization_id: i32,
	pub slug: String,
	pub name: String,
	pub description: String,
	pub repository_count: i64,
}

/// What a new or edited project contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDraft {
	pub slug: String,
	pub name: String,
	pub description: String,
	pub repositories: Vec<ProjectRepositoryRecord>,
}

fn now() -> String {
	chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Inserts or refreshes an organisation and links the installation to it.
///
/// Called whenever the provider tells monochange about an installation's
/// account, so logins and avatars follow renames.
pub async fn link_installation_organization(
	executor: &mut sqlx::SqliteConnection,
	installation_id: i32,
	account: &AccountIdentity,
) -> Result<i32, sqlx::Error> {
	let organization_id: i32 = sqlx::query_scalar(
		"INSERT INTO organizations (provider, github_id, github_login, github_avatar_url, account_type, created_at, updated_at)
		 VALUES ($1, $2, $3, $4, $5, $6, $6)
		 ON CONFLICT(provider, github_id) DO UPDATE SET
		     github_login = excluded.github_login,
		     github_avatar_url = COALESCE(excluded.github_avatar_url, organizations.github_avatar_url),
		     account_type = excluded.account_type,
		     updated_at = excluded.updated_at
		 RETURNING id",
	)
	.bind(&account.provider)
	.bind(account.external_id)
	.bind(&account.login)
	.bind(&account.avatar_url)
	.bind(&account.account_type)
	.bind(now())
	.fetch_one(&mut *executor)
	.await?;
	sqlx::query("UPDATE installations SET organization_id = $2 WHERE id = $1")
		.bind(installation_id)
		.bind(organization_id)
		.execute(&mut *executor)
		.await?;
	Ok(organization_id)
}

fn organization_from_row(row: &sqlx::sqlite::SqliteRow) -> OrganizationRecord {
	OrganizationRecord {
		id: row.get("id"),
		provider: row.get("provider"),
		external_id: row.get("github_id"),
		login: row.get("github_login"),
		account_type: row.get("account_type"),
		avatar_url: row.get("github_avatar_url"),
	}
}

pub async fn find_organization(
	pool: &DbPool,
	provider: &str,
	login: &str,
) -> Result<Option<OrganizationRecord>, sqlx::Error> {
	let row = sqlx::query(
		"SELECT id, provider, github_id, github_login, account_type, github_avatar_url
		 FROM organizations WHERE provider = $1 AND github_login = $2 COLLATE NOCASE",
	)
	.bind(provider)
	.bind(login)
	.fetch_optional(pool)
	.await?;
	Ok(row.as_ref().map(organization_from_row))
}

fn project_from_row(row: &sqlx::sqlite::SqliteRow) -> ProjectRecord {
	ProjectRecord {
		id: row.get("id"),
		organization_id: row.get("organization_id"),
		slug: row.get("slug"),
		name: row.get("name"),
		description: row.get("description"),
		repository_count: row.get("repository_count"),
	}
}

const PROJECT_COLUMNS: &str = "p.id, p.organization_id, p.slug, p.name, p.description,
	(SELECT COUNT(*) FROM project_repositories r WHERE r.project_id = p.id) AS repository_count";

pub async fn list_projects(
	pool: &DbPool,
	organization_id: i32,
) -> Result<Vec<ProjectRecord>, sqlx::Error> {
	let rows = sqlx::query(&format!(
		"SELECT {PROJECT_COLUMNS} FROM projects p WHERE p.organization_id = $1 ORDER BY p.name COLLATE NOCASE"
	))
	.bind(organization_id)
	.fetch_all(pool)
	.await?;
	Ok(rows.iter().map(project_from_row).collect())
}

pub async fn find_project(
	pool: &DbPool,
	organization_id: i32,
	slug: &str,
) -> Result<Option<ProjectRecord>, sqlx::Error> {
	let row = sqlx::query(&format!(
		"SELECT {PROJECT_COLUMNS} FROM projects p WHERE p.organization_id = $1 AND p.slug = $2"
	))
	.bind(organization_id)
	.bind(slug)
	.fetch_optional(pool)
	.await?;
	Ok(row.as_ref().map(project_from_row))
}

pub async fn project_repositories(
	pool: &DbPool,
	project_id: i32,
) -> Result<Vec<ProjectRepositoryRecord>, sqlx::Error> {
	let rows = sqlx::query(
		"SELECT provider, repository_external_id, full_name FROM project_repositories
		 WHERE project_id = $1 ORDER BY full_name COLLATE NOCASE",
	)
	.bind(project_id)
	.fetch_all(pool)
	.await?;
	Ok(rows
		.iter()
		.map(|row| {
			ProjectRepositoryRecord {
				provider: row.get("provider"),
				external_id: row.get("repository_external_id"),
				full_name: row.get("full_name"),
			}
		})
		.collect())
}

fn slug_conflict(error: sqlx::Error, slug: &str) -> ProjectStoreError {
	if error
		.as_database_error()
		.is_some_and(sqlx::error::DatabaseError::is_unique_violation)
	{
		ProjectStoreError::SlugTaken(slug.to_owned())
	} else {
		ProjectStoreError::Database(error)
	}
}

async fn replace_repositories(
	executor: &mut sqlx::SqliteConnection,
	project_id: i32,
	repositories: &[ProjectRepositoryRecord],
) -> Result<(), sqlx::Error> {
	sqlx::query("DELETE FROM project_repositories WHERE project_id = $1")
		.bind(project_id)
		.execute(&mut *executor)
		.await?;
	for repository in repositories {
		sqlx::query(
			"INSERT INTO project_repositories (project_id, provider, repository_external_id, full_name, created_at)
			 VALUES ($1, $2, $3, $4, $5)",
		)
		.bind(project_id)
		.bind(&repository.provider)
		.bind(repository.external_id)
		.bind(&repository.full_name)
		.bind(now())
		.execute(&mut *executor)
		.await?;
	}
	Ok(())
}

/// Creates a project and its repository links in one transaction.
pub async fn create_project(
	pool: &DbPool,
	organization_id: i32,
	created_by_user_id: i32,
	draft: &ProjectDraft,
) -> Result<i32, ProjectStoreError> {
	let mut transaction = pool.begin().await?;
	let project_id: i32 = sqlx::query_scalar(
		"INSERT INTO projects (organization_id, slug, name, description, created_by_user_id, created_at, updated_at)
		 VALUES ($1, $2, $3, $4, $5, $6, $6) RETURNING id",
	)
	.bind(organization_id)
	.bind(&draft.slug)
	.bind(&draft.name)
	.bind(&draft.description)
	.bind(created_by_user_id)
	.bind(now())
	.fetch_one(&mut *transaction)
	.await
	.map_err(|error| slug_conflict(error, &draft.slug))?;
	replace_repositories(&mut transaction, project_id, &draft.repositories).await?;
	transaction.commit().await?;
	Ok(project_id)
}

/// Renames a project and replaces its repositories in one transaction.
pub async fn update_project(
	pool: &DbPool,
	project_id: i32,
	draft: &ProjectDraft,
) -> Result<(), ProjectStoreError> {
	let mut transaction = pool.begin().await?;
	sqlx::query(
		"UPDATE projects SET slug = $2, name = $3, description = $4, updated_at = $5 WHERE id = $1",
	)
	.bind(project_id)
	.bind(&draft.slug)
	.bind(&draft.name)
	.bind(&draft.description)
	.bind(now())
	.execute(&mut *transaction)
	.await
	.map_err(|error| slug_conflict(error, &draft.slug))?;
	replace_repositories(&mut transaction, project_id, &draft.repositories).await?;
	transaction.commit().await?;
	Ok(())
}

pub async fn delete_project(pool: &DbPool, project_id: i32) -> Result<(), sqlx::Error> {
	sqlx::query("DELETE FROM projects WHERE id = $1")
		.bind(project_id)
		.execute(pool)
		.await?;
	Ok(())
}

#[cfg(test)]
#[path = "__tests__/projects_tests.rs"]
mod tests;
