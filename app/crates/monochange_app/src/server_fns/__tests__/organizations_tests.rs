//! Organisations and projects are scoped to the repositories a user manages.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::sync::OnceLock;

use axum::http::Request;
use axum::http::header::COOKIE;
use axum::http::header::LOCATION;
use httpmock::Method::GET;
use httpmock::MockServer;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use leptos_axum::ResponseOptions;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::create_token;
use monochange_app_api::github_app::GitHubAppAuth;
use monochange_app_api::oauth;
use monochange_app_db::projects::AccountIdentity;
use monochange_app_db::projects::GITHUB;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;

use super::create_project;
use super::delete_project;
use super::list_organizations;
use super::organization_overview;
use super::project_overview;
use super::update_project;

/// Alice owns her personal account and the `team` organisation; Bob's
/// installation predates organisations and is not linked yet.
async fn state(server: &MockServer) -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token'), (2, 202, 'bob', 'test-only-token')").execute(&db).await.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'alice', 'User'), (2, 2, 1002, 'bob', 'User'), (3, 1, 1003, 'team', 'Organization')").execute(&db).await.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'alice/api', 0), (1, 12, 'alice/web', 1), (2, 21, 'bob/private', 1), (3, 31, 'team/app', 1)").execute(&db).await.unwrap();
	let mut connection = db.acquire().await.unwrap();
	for (installation, external_id, login, account_type) in
		[(1, 101, "alice", "User"), (3, 5001, "team", "Organization")]
	{
		monochange_app_db::projects::link_installation_organization(
			&mut connection,
			installation,
			&AccountIdentity {
				provider: GITHUB.to_owned(),
				external_id,
				login: login.to_owned(),
				account_type: account_type.to_owned(),
				avatar_url: None,
			},
		)
		.await
		.unwrap();
	}
	drop(connection);
	let secrets: AppSecrets =
		serde_json::from_value(serde_json::json!({"jwt_secret":"organization-test-signing-key"}))
			.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_api_origin = server.base_url();
	Arc::new(state)
}

async fn owner_of_team(server: &MockServer) {
	server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/team");
			then.json_body(serde_json::json!({"state":"active","role":"admin"}));
		})
		.await;
}

fn context(state: &Arc<AppState>, user_id: Option<i32>) -> (Owner, ResponseOptions) {
	let owner = Owner::new();
	let response = ResponseOptions::default();
	let mut request = Request::new(());
	if let Some(user_id) = user_id {
		let (github_id, login) = if user_id == 1 {
			(101, "alice")
		} else {
			(202, "bob")
		};
		let token = create_token(&state.jwt_secret, user_id, github_id, login).unwrap();
		request.headers_mut().insert(
			COOKIE,
			format!("{}={token}", oauth::SESSION_COOKIE_NAME)
				.parse()
				.unwrap(),
		);
	}
	let (parts, ()) = request.into_parts();
	owner.with(|| {
		provide_context(state.clone());
		provide_context(parts);
		provide_context(response.clone());
	});
	(owner, response)
}

fn location(response: &ResponseOptions) -> Option<String> {
	response
		.0
		.read()
		.unwrap()
		.headers
		.get(LOCATION)
		.map(|value| value.to_str().unwrap().to_owned())
}

fn repositories(names: &[&str]) -> Vec<String> {
	names.iter().map(|name| (*name).to_owned()).collect()
}

async fn create(
	owner: &Owner,
	organization: &str,
	name: &str,
	names: &[&str],
) -> Result<(), server_fn::ServerFnError> {
	owner
		.with(|| {
			ScopedFuture::new(create_project(
				organization.to_owned(),
				name.to_owned(),
				"Billing apps".to_owned(),
				repositories(names),
			))
		})
		.await
}

fn message(error: &server_fn::ServerFnError) -> String {
	crate::pages::organization::action_error(error)
}

#[tokio::test]
async fn signed_out_visitors_have_no_organisations() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (owner, _) = context(&state, None);
	assert!(
		owner
			.with(|| ScopedFuture::new(list_organizations()))
			.await
			.unwrap()
			.is_empty()
	);
	assert!(
		owner
			.with(|| ScopedFuture::new(organization_overview("alice".to_owned())))
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn organisations_group_the_repositories_a_user_manages() {
	let server = MockServer::start_async().await;
	owner_of_team(&server).await;
	let state = state(&server).await;
	let (owner, _) = context(&state, Some(1));
	let organizations = owner
		.with(|| ScopedFuture::new(list_organizations()))
		.await
		.unwrap();
	let summary: Vec<_> = organizations
		.iter()
		.map(|organization| {
			(
				organization.login.as_str(),
				organization.personal,
				organization.supports_projects,
				organization.repositories.len(),
			)
		})
		.collect();
	assert_eq!(
		summary,
		[("alice", true, true, 2), ("team", false, true, 1)]
	);
	assert_eq!(organizations[0].path(), "/dashboard/alice");
	assert!(
		organizations
			.iter()
			.all(|organization| organization.login != "bob")
	);
}

#[tokio::test]
async fn projects_are_created_viewed_edited_and_deleted() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (owner, response) = context(&state, Some(1));

	create(
		&owner,
		"Alice",
		"Invoices & Billing",
		&["alice/web", "ALICE/API"],
	)
	.await
	.unwrap();
	assert_eq!(
		location(&response).as_deref(),
		Some("/dashboard/alice/projects/invoices-billing")
	);

	let overview = owner
		.with(|| ScopedFuture::new(organization_overview("alice".to_owned())))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(overview.projects.len(), 1);
	assert_eq!(overview.projects[0].name, "Invoices & Billing");
	assert_eq!(overview.projects[0].repository_count, 2);

	let details = owner
		.with(|| {
			ScopedFuture::new(project_overview(
				"alice".to_owned(),
				"invoices-billing".to_owned(),
			))
		})
		.await
		.unwrap()
		.unwrap();
	assert_eq!(details.organization, "alice");
	assert!(details.personal);
	assert_eq!(details.project.description, "Billing apps");
	let names: Vec<_> = details
		.repositories
		.iter()
		.map(|repository| (repository.full_name.as_str(), repository.private))
		.collect();
	assert_eq!(
		names,
		[("alice/api", Some(false)), ("alice/web", Some(true))]
	);
	assert_eq!(details.available.len(), 2);

	owner
		.with(|| {
			ScopedFuture::new(update_project(
				"alice".to_owned(),
				"invoices-billing".to_owned(),
				"Web".to_owned(),
				String::new(),
				repositories(&["alice/web"]),
			))
		})
		.await
		.unwrap();
	assert_eq!(
		location(&response).as_deref(),
		Some("/dashboard/alice/projects/web")
	);
	let renamed = owner
		.with(|| ScopedFuture::new(project_overview("alice".to_owned(), "web".to_owned())))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(renamed.repositories.len(), 1);
	assert_eq!(renamed.project.description, "");

	owner
		.with(|| ScopedFuture::new(delete_project("alice".to_owned(), "web".to_owned())))
		.await
		.unwrap();
	assert_eq!(location(&response).as_deref(), Some("/dashboard/alice"));
	assert!(
		owner
			.with(|| ScopedFuture::new(project_overview("alice".to_owned(), "web".to_owned())))
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn project_forms_explain_what_went_wrong() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (owner, _) = context(&state, Some(1));

	let cases: [(&str, &str, &[&str], &str); 4] = [
		("alice", "Invoices", &[], "Choose at least one repository."),
		(
			"alice",
			"Invoices",
			&["bob/private"],
			"bob/private isn't a repository you can add to this project.",
		),
		(
			"bob",
			"Invoices",
			&["bob/private"],
			"That organisation or project isn't available to you.",
		),
		("alice", " ", &["alice/api"], "Give the project a name."),
	];
	for (organization, name, names, expected) in cases {
		let error = create(&owner, organization, name, names).await.unwrap_err();
		assert_eq!(message(&error), expected, "{organization} {name}");
	}

	create(&owner, "alice", "Invoices", &["alice/api"])
		.await
		.unwrap();
	let taken = create(&owner, "alice", "invoices!", &["alice/api"])
		.await
		.unwrap_err();
	assert_eq!(
		message(&taken),
		"A project at /invoices already exists. Choose a different name."
	);
}

#[tokio::test]
async fn missing_projects_and_other_peoples_organisations_are_hidden() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (alice, _) = context(&state, Some(1));
	create(&alice, "alice", "Invoices", &["alice/api"])
		.await
		.unwrap();

	let (bob, _) = context(&state, Some(2));
	assert!(
		bob.with(|| ScopedFuture::new(project_overview("alice".to_owned(), "invoices".to_owned())))
			.await
			.unwrap()
			.is_none()
	);
	for result in [
		bob.with(|| {
			ScopedFuture::new(update_project(
				"alice".to_owned(),
				"invoices".to_owned(),
				"Mine".to_owned(),
				String::new(),
				repositories(&["bob/private"]),
			))
		})
		.await,
		bob.with(|| ScopedFuture::new(delete_project("alice".to_owned(), "invoices".to_owned())))
			.await,
	] {
		assert_eq!(
			message(&result.unwrap_err()),
			"That organisation or project isn't available to you."
		);
	}
	for result in [
		alice
			.with(|| {
				ScopedFuture::new(update_project(
					"alice".to_owned(),
					"missing".to_owned(),
					"Missing".to_owned(),
					String::new(),
					repositories(&["alice/api"]),
				))
			})
			.await,
		alice
			.with(|| ScopedFuture::new(delete_project("alice".to_owned(), "missing".to_owned())))
			.await,
	] {
		assert_eq!(
			message(&result.unwrap_err()),
			"That organisation or project isn't available to you."
		);
	}
	assert!(
		alice
			.with(|| ScopedFuture::new(project_overview("alice".to_owned(), "missing".to_owned())))
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn disconnected_repositories_keep_their_last_known_name() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (owner, _) = context(&state, Some(1));
	create(&owner, "alice", "Invoices", &["alice/api", "alice/web"])
		.await
		.unwrap();
	sqlx::query("DELETE FROM repositories WHERE github_repo_id = 12")
		.execute(&state.db)
		.await
		.unwrap();
	let details = owner
		.with(|| ScopedFuture::new(project_overview("alice".to_owned(), "invoices".to_owned())))
		.await
		.unwrap()
		.unwrap();
	let names: Vec<_> = details
		.repositories
		.iter()
		.map(|repository| (repository.full_name.as_str(), repository.private))
		.collect();
	assert_eq!(names, [("alice/api", Some(false)), ("alice/web", None)]);
}

fn test_app(server: &MockServer) -> GitHubAppAuth {
	static KEY: OnceLock<String> = OnceLock::new();
	let pem = KEY.get_or_init(|| {
		RsaPrivateKey::new(&mut OsRng, 2048)
			.unwrap()
			.to_pkcs1_pem(LineEnding::LF)
			.unwrap()
			.to_string()
	});
	GitHubAppAuth::new("123", pem, "test-only-webhook-secret", &server.base_url())
}

#[tokio::test]
async fn unlinked_installations_wait_for_github_before_offering_projects() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (bob, _) = context(&state, Some(2));
	let organizations = bob
		.with(|| ScopedFuture::new(list_organizations()))
		.await
		.unwrap();
	assert_eq!(organizations.len(), 1);
	assert!(!organizations[0].supports_projects);
	assert!(organizations[0].projects.is_empty());
	assert!(
		bob.with(|| ScopedFuture::new(project_overview("bob".to_owned(), "any".to_owned())))
			.await
			.unwrap()
			.is_none()
	);
	let error = create(&bob, "bob", "Private", &["bob/private"])
		.await
		.unwrap_err();
	assert_eq!(
		message(&error),
		"monochange is still confirming this account with GitHub. Reload the page and try again."
	);
}

#[tokio::test]
async fn unlinked_installations_are_linked_through_the_github_app() {
	let server = MockServer::start_async().await;
	let lookup = server
		.mock_async(|when, then| {
			when.method(GET).path("/app/installations/1002");
			then.json_body(serde_json::json!({
				"account": {"id": 202, "login": "bob", "type": "User", "avatar_url": "https://avatars/bob.png"}
			}));
		})
		.await;
	let mut configured = (*state(&server).await).clone();
	configured.github_app = Some(test_app(&server));
	let state = Arc::new(configured);
	let (bob, _) = context(&state, Some(2));
	let organizations = bob
		.with(|| ScopedFuture::new(list_organizations()))
		.await
		.unwrap();
	assert!(organizations[0].supports_projects);
	assert_eq!(
		organizations[0].avatar_url.as_deref(),
		Some("https://avatars/bob.png")
	);
	lookup.assert_calls_async(1).await;

	// Once linked, later visits don't ask GitHub again.
	bob.with(|| ScopedFuture::new(list_organizations()))
		.await
		.unwrap();
	lookup.assert_calls_async(1).await;
}

#[tokio::test]
async fn failed_github_lookups_leave_the_installation_for_the_next_visit() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/app/installations/1002");
			then.status(500);
		})
		.await;
	let mut configured = (*state(&server).await).clone();
	configured.github_app = Some(test_app(&server));
	let state = Arc::new(configured);
	let (bob, _) = context(&state, Some(2));
	let organizations = bob
		.with(|| ScopedFuture::new(list_organizations()))
		.await
		.unwrap();
	assert!(!organizations[0].supports_projects);
}

#[tokio::test]
async fn database_failures_surface_as_errors() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let (owner, _) = context(&state, Some(1));
	create(&owner, "alice", "Invoices", &["alice/api"])
		.await
		.unwrap();
	sqlx::query("DROP TABLE project_repositories")
		.execute(&state.db)
		.await
		.unwrap();
	for result in [
		owner
			.with(|| ScopedFuture::new(project_overview("alice".to_owned(), "invoices".to_owned())))
			.await
			.map(|_| ()),
		owner
			.with(|| ScopedFuture::new(delete_project("alice".to_owned(), "invoices".to_owned())))
			.await,
		create(&owner, "alice", "Other", &["alice/api"]).await,
	] {
		assert!(message(&result.unwrap_err()).starts_with("DB: "));
	}
	sqlx::query("DROP TABLE projects")
		.execute(&state.db)
		.await
		.unwrap();
	assert!(
		message(
			&owner
				.with(|| ScopedFuture::new(list_organizations()))
				.await
				.unwrap_err()
		)
		.starts_with("DB: ")
	);
}

#[test]
fn transport_errors_are_shown_as_they_are() {
	let error = server_fn::ServerFnError::Request("offline".to_owned());
	assert_eq!(
		crate::pages::organization::action_error(&error),
		error.to_string()
	);
}
