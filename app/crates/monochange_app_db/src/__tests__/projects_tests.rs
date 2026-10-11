//! Organisations and projects round-trip through the migrated schema.

// `#[tokio::test]` builds its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use super::AccountIdentity;
use super::GITHUB;
use super::ProjectDraft;
use super::ProjectRepositoryRecord;
use super::ProjectStoreError;
use super::create_project;
use super::delete_project;
use super::find_organization;
use super::find_project;
use super::link_installation_organization;
use super::list_projects;
use super::project_repositories;
use super::update_project;
use crate::DbPool;

async fn pool() -> DbPool {
	let pool = crate::create_pool("sqlite::memory:").await.unwrap();
	crate::run_migrations(&pool).await.unwrap();
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', '')")
		.execute(&pool)
		.await
		.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'acme', 'Organization'), (2, 1, 1002, 'acme', 'Organization')")
		.execute(&pool)
		.await
		.unwrap();
	pool
}

fn acme(login: &str, avatar: Option<&str>) -> AccountIdentity {
	AccountIdentity {
		provider: GITHUB.to_owned(),
		external_id: 5001,
		login: login.to_owned(),
		account_type: "Organization".to_owned(),
		avatar_url: avatar.map(str::to_owned),
	}
}

async fn link(pool: &DbPool, installation_id: i32, account: &AccountIdentity) -> i32 {
	let mut connection = pool.acquire().await.unwrap();
	link_installation_organization(&mut connection, installation_id, account)
		.await
		.unwrap()
}

fn repository(external_id: i64, full_name: &str) -> ProjectRepositoryRecord {
	ProjectRepositoryRecord {
		provider: GITHUB.to_owned(),
		external_id,
		full_name: full_name.to_owned(),
	}
}

fn draft(slug: &str, repositories: Vec<ProjectRepositoryRecord>) -> ProjectDraft {
	ProjectDraft {
		slug: slug.to_owned(),
		name: "Invoices".to_owned(),
		description: "Billing apps".to_owned(),
		repositories,
	}
}

#[tokio::test]
async fn installations_share_one_organisation_that_follows_renames() {
	let pool = pool().await;
	let first = link(&pool, 1, &acme("acme", Some("https://avatars/acme.png"))).await;
	// A reinstall or rename keeps the same organisation row and its avatar.
	let second = link(&pool, 2, &acme("acme-co", None)).await;
	assert_eq!(first, second);

	let organization = find_organization(&pool, GITHUB, "ACME-CO")
		.await
		.unwrap()
		.unwrap();
	assert_eq!(organization.id, first);
	assert_eq!(organization.external_id, 5001);
	assert_eq!(organization.login, "acme-co");
	assert_eq!(organization.account_type, "Organization");
	assert_eq!(
		organization.avatar_url.as_deref(),
		Some("https://avatars/acme.png")
	);
	assert_eq!(organization.provider, GITHUB);
	assert!(
		find_organization(&pool, GITHUB, "acme")
			.await
			.unwrap()
			.is_none()
	);

	let linked: Vec<Option<i32>> =
		sqlx::query_scalar("SELECT organization_id FROM installations ORDER BY id")
			.fetch_all(&pool)
			.await
			.unwrap();
	assert_eq!(linked, [Some(first), Some(first)]);
}

#[tokio::test]
async fn projects_are_created_listed_updated_and_deleted() {
	let pool = pool().await;
	let organization = link(&pool, 1, &acme("acme", None)).await;
	let id = create_project(
		&pool,
		organization,
		1,
		&draft(
			"invoices",
			vec![repository(12, "acme/web"), repository(11, "acme/api")],
		),
	)
	.await
	.unwrap();

	let project = find_project(&pool, organization, "invoices")
		.await
		.unwrap()
		.unwrap();
	assert_eq!(project.id, id);
	assert_eq!(project.name, "Invoices");
	assert_eq!(project.description, "Billing apps");
	assert_eq!(project.repository_count, 2);
	let names: Vec<_> = project_repositories(&pool, id)
		.await
		.unwrap()
		.into_iter()
		.map(|repository| repository.full_name)
		.collect();
	assert_eq!(names, ["acme/api", "acme/web"]);

	let mut renamed = draft("billing", vec![repository(11, "acme/api")]);
	renamed.name = "Billing".to_owned();
	update_project(&pool, id, &renamed).await.unwrap();
	let projects = list_projects(&pool, organization).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].slug, "billing");
	assert_eq!(projects[0].repository_count, 1);
	assert!(
		find_project(&pool, organization, "invoices")
			.await
			.unwrap()
			.is_none()
	);

	delete_project(&pool, id).await.unwrap();
	assert!(list_projects(&pool, organization).await.unwrap().is_empty());
	let orphaned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_repositories")
		.fetch_one(&pool)
		.await
		.unwrap();
	assert_eq!(orphaned, 0);
}

#[tokio::test]
async fn slugs_are_unique_within_an_organisation() {
	let pool = pool().await;
	let organization = link(&pool, 1, &acme("acme", None)).await;
	create_project(&pool, organization, 1, &draft("invoices", vec![]))
		.await
		.unwrap();
	let duplicate = create_project(&pool, organization, 1, &draft("invoices", vec![]))
		.await
		.unwrap_err();
	assert!(matches!(duplicate, ProjectStoreError::SlugTaken(ref slug) if slug == "invoices"));
	assert_eq!(
		duplicate.to_string(),
		"a project with the slug `invoices` already exists in this organisation"
	);

	let other = create_project(&pool, organization, 1, &draft("other", vec![]))
		.await
		.unwrap();
	let clash = update_project(&pool, other, &draft("invoices", vec![]))
		.await
		.unwrap_err();
	assert!(matches!(clash, ProjectStoreError::SlugTaken(_)));
}

#[tokio::test]
async fn projects_outlive_a_reinstall() {
	let pool = pool().await;
	let organization = link(&pool, 1, &acme("acme", None)).await;
	let id = create_project(
		&pool,
		organization,
		1,
		&draft("invoices", vec![repository(11, "acme/api")]),
	)
	.await
	.unwrap();
	sqlx::query("DELETE FROM installations")
		.execute(&pool)
		.await
		.unwrap();
	let repositories = project_repositories(&pool, id).await.unwrap();
	assert_eq!(repositories, [repository(11, "acme/api")]);
}

#[tokio::test]
async fn database_failures_are_not_mistaken_for_slug_clashes() {
	let pool = pool().await;
	// No organisation with id 999 exists, so the foreign key rejects it.
	let error = create_project(&pool, 999, 1, &draft("invoices", vec![]))
		.await
		.unwrap_err();
	assert!(matches!(error, ProjectStoreError::Database(_)));
	pool.close().await;
	assert!(matches!(
		update_project(&pool, 1, &draft("x", vec![])).await,
		Err(ProjectStoreError::Database(_))
	));
}
