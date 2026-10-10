//! Project feedback persists through optimistic, retried updates.

#![allow(clippy::disallowed_methods)]

use std::sync::OnceLock;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use httpmock::Method::GET;
use httpmock::Method::POST;
use httpmock::MockServer;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::FeedbackSubmission;
use monochange_app_feedback::MaintainerDecision;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::ServiceError;
use monochange_app_feedback::Stage;
use monochange_app_feedback::SubmitterIdentity;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;

use super::FeedbackError;
use super::PORTAL_APP;
use super::ProjectRepository;
use super::ProjectScope;
use super::create_issue;
use super::load;
use super::notifications_for;
use super::public_project;
use super::update;
use super::viewer_id;
use crate::AppSecrets;
use crate::AppState;
use crate::github_app::GitHubAppAuth;

async fn state() -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	sqlx::query("INSERT INTO organizations (id, github_id, github_login) VALUES (1, 5001, 'acme')")
		.execute(&db)
		.await
		.unwrap();
	sqlx::query(
		"INSERT INTO projects (id, organization_id, slug, name) VALUES (1, 1, 'invoices', 'Invoices')",
	)
	.execute(&db)
	.await
	.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({})).unwrap();
	AppState::new(db, secrets).unwrap()
}

fn scope(private_web: bool) -> ProjectScope {
	ProjectScope {
		project_id: 1,
		repositories: vec![
			ProjectRepository {
				full_name: "acme/api".to_owned(),
				private: false,
				github_installation_id: 1001,
			},
			ProjectRepository {
				full_name: "acme/web".to_owned(),
				private: private_web,
				github_installation_id: 1001,
			},
		],
		disconnected: 0,
	}
}

fn submission(description: &str) -> FeedbackSubmission {
	FeedbackSubmission {
		kind: FeedbackKind::BugReport,
		description: description.to_owned(),
		page: Some(monochange_app_feedback::PageContext {
			route: "/invoices".to_owned(),
			app_version: Some("1.0.0".to_owned()),
			locale: None,
			element: None,
		}),
		attachments: vec![monochange_app_feedback::Attachment::Screenshot {
			media_id: "m".to_owned(),
		}],
		submitter: SubmitterIdentity {
			anonymous_id: "anon-1".to_owned(),
			email: None,
		},
		app_slug: PORTAL_APP.to_owned(),
	}
}

fn decision() -> MaintainerDecision {
	MaintainerDecision {
		maintainer: "ifiok".to_owned(),
		rationale: "Fits".to_owned(),
		override_vote_threshold: Some("Important".to_owned()),
	}
}

#[tokio::test]
async fn new_projects_start_with_their_portal_and_strictest_visibility() {
	let state = state().await;
	let public = load(&state, &scope(false)).await.unwrap();
	assert!(public.service.app(PORTAL_APP).is_some());
	assert!(public.service.items().is_empty());
	assert_eq!(scope(false).visibility(), RepositoryVisibility::Public);
	let mut unknown = scope(false);
	unknown.disconnected = 1;
	assert_eq!(unknown.visibility(), RepositoryVisibility::Private);
	let private = load(&state, &scope(true)).await.unwrap();
	assert_eq!(
		private.service.policy().visibility,
		RepositoryVisibility::Private
	);
}

#[tokio::test]
async fn updates_persist_items_and_notifications() {
	let state = state().await;
	let scope = scope(false);
	let receipt = update(&state, &scope, |service| {
		service.receive(submission("Totals are off"))
	})
	.await
	.unwrap();
	assert_eq!(receipt.id, "fb-1");
	assert_eq!(receipt.stage, Stage::Voting);

	update(&state, &scope, |service| service.accept("fb-1", decision()))
		.await
		.unwrap();
	let reloaded = load(&state, &scope).await.unwrap();
	assert_eq!(
		reloaded.service.item("fb-1").unwrap().stage,
		Stage::Accepted
	);
	let notifications = notifications_for(&state, &scope, "anon-1", 10)
		.await
		.unwrap();
	assert_eq!(notifications.len(), 1);
	assert_eq!(notifications[0].update.body, "Accepted and planned");
}

#[tokio::test]
async fn failed_operations_save_nothing() {
	let state = state().await;
	let scope = scope(false);
	let error = update(&state, &scope, |service| service.vote("fb-404", "anon-2"))
		.await
		.unwrap_err();
	assert!(matches!(
		error,
		FeedbackError::Service(ServiceError::NotFound(_))
	));
	assert!(
		monochange_app_db::feedback::load_feedback(&state.db, 1)
			.await
			.unwrap()
			.is_none()
	);
}

/// Saves a newer document behind the operation's back, as a concurrent
/// request would.
fn cut_in(state: &AppState) {
	let pool = state.db.clone();
	tokio::task::block_in_place(|| {
		tokio::runtime::Handle::current().block_on(async move {
			let stored = monochange_app_db::feedback::load_feedback(&pool, 1)
				.await
				.unwrap();
			monochange_app_db::feedback::save_feedback(
				&pool,
				1,
				stored.as_ref().map_or(
					"{\"rules\":{\"acceptance_threshold\":3},\"apps\":[],\"items\":[]}",
					|stored| stored.state_json.as_str(),
				),
				stored.as_ref().map(|stored| stored.version),
				&[],
			)
			.await
			.unwrap();
		});
	});
}

#[tokio::test(flavor = "multi_thread")]
async fn lost_races_rerun_against_the_newer_document() {
	let state = state().await;
	let scope = scope(false);
	let attempts = AtomicUsize::new(0);
	let receipt = update(&state, &scope, |service| {
		if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
			cut_in(&state);
		}
		service.receive(submission("Totals are off"))
	})
	.await
	.unwrap();
	assert_eq!(attempts.load(Ordering::SeqCst), 2);
	assert_eq!(receipt.id, "fb-1");
}

#[tokio::test(flavor = "multi_thread")]
async fn constant_contention_gives_up_with_a_retryable_error() {
	let state = state().await;
	let scope = scope(false);
	let error = update(&state, &scope, |service| {
		cut_in(&state);
		service.receive(submission("Totals are off"))
	})
	.await
	.unwrap_err();
	assert!(matches!(error, FeedbackError::Busy));
	assert_eq!(
		error.to_string(),
		"feedback is changing quickly right now; try again"
	);
}

#[tokio::test]
async fn corrupt_documents_are_reported_not_replaced() {
	let state = state().await;
	monochange_app_db::feedback::save_feedback(&state.db, 1, "not json", None, &[])
		.await
		.unwrap();
	assert!(matches!(
		load(&state, &scope(false)).await,
		Err(FeedbackError::Corrupt(_))
	));
	sqlx::query("INSERT INTO feedback_notifications (project_id, recipient, item_id, notification_json) VALUES (1, 'anon-1', 'fb-1', 'nope')")
		.execute(&state.db)
		.await
		.unwrap();
	assert!(matches!(
		notifications_for(&state, &scope(false), "anon-1", 5).await,
		Err(FeedbackError::Corrupt(_))
	));
}

#[tokio::test]
async fn storage_failures_surface() {
	let state = state().await;
	state.db.close().await;
	assert!(matches!(
		load(&state, &scope(false)).await,
		Err(FeedbackError::Store(_))
	));
}

fn app(server: &MockServer) -> GitHubAppAuth {
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

async fn accepted(state: &AppState, scope: &ProjectScope) {
	update(state, scope, |service| {
		service.receive(submission("Crash in /app/src/totals.rs"))
	})
	.await
	.unwrap();
	update(state, scope, |service| service.accept("fb-1", decision()))
		.await
		.unwrap();
}

#[tokio::test]
async fn accepted_items_open_and_link_a_github_issue() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(POST)
				.path("/app/installations/1001/access_tokens");
			then.status(201)
				.json_body(serde_json::json!({"token": "test-only-installation-token"}));
		})
		.await;
	let issue = server
		.mock_async(|when, then| {
			when.method(POST)
				.path("/repos/acme/web/issues")
				.header("authorization", "Bearer test-only-installation-token")
				.body_includes("\"labels\":[\"feedback\",\"bug\"]")
				// The private repository's issue keeps internal paths.
				.body_includes("/app/src/totals.rs");
			then.status(201).json_body(
				serde_json::json!({"number": 77, "html_url": "https://github.com/acme/web/issues/77"}),
			);
		})
		.await;
	let mut state = state().await;
	state.github_app = Some(app(&server));
	let scope = scope(true);
	accepted(&state, &scope).await;

	let linked = create_issue(&state, &scope, "fb-1", "ACME/web")
		.await
		.unwrap();
	issue.assert_async().await;
	assert_eq!(linked.repository.as_deref(), Some("acme/web"));
	assert_eq!(linked.number, 77);
	let reloaded = load(&state, &scope).await.unwrap();
	assert_eq!(reloaded.service.item("fb-1").unwrap().issue, Some(linked));
}

#[tokio::test]
async fn issue_creation_explains_why_it_cannot_run() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(POST)
				.path("/app/installations/1001/access_tokens");
			then.status(201)
				.json_body(serde_json::json!({"token": "t"}));
		})
		.await;
	server
		.mock_async(|when, then| {
			when.method(POST).path("/repos/acme/api/issues");
			then.status(403)
				.body("Resource not accessible by integration");
		})
		.await;
	let mut state = state().await;
	let scope = scope(false);
	accepted(&state, &scope).await;

	let missing = create_issue(&state, &scope, "fb-1", "acme/api")
		.await
		.unwrap_err();
	assert_eq!(
		missing.to_string(),
		"the GitHub App isn't configured on this deployment"
	);
	state.github_app = Some(app(&server));
	let unknown = create_issue(&state, &scope, "fb-1", "acme/other")
		.await
		.unwrap_err();
	assert_eq!(
		unknown.to_string(),
		"acme/other isn't one of this project's connected repositories"
	);
	let not_accepted = create_issue(&state, &scope, "fb-404", "acme/api")
		.await
		.unwrap_err();
	assert!(matches!(not_accepted, FeedbackError::Service(_)));
	let forbidden = create_issue(&state, &scope, "fb-1", "acme/api")
		.await
		.unwrap_err();
	assert!(matches!(forbidden, FeedbackError::GitHub(_)));
	// Nothing was linked, so the maintainer can retry.
	assert!(
		load(&state, &scope)
			.await
			.unwrap()
			.service
			.item("fb-1")
			.unwrap()
			.issue
			.is_none()
	);
}

#[tokio::test]
async fn issue_creation_fails_cleanly_without_an_installation_token() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/never-called");
			then.status(200);
		})
		.await;
	let mut state = state().await;
	state.github_app = Some(app(&server));
	let scope = scope(false);
	accepted(&state, &scope).await;
	let error = create_issue(&state, &scope, "fb-1", "acme/api")
		.await
		.unwrap_err();
	assert!(matches!(error, FeedbackError::GitHub(_)));
}

#[test]
fn errors_explain_themselves() {
	assert_eq!(
		FeedbackError::Service(ServiceError::NotFound("fb-1".to_owned())).to_string(),
		"feedback item fb-1 does not exist"
	);
	assert!(
		FeedbackError::Corrupt(serde_json::from_str::<u8>("x").unwrap_err())
			.to_string()
			.starts_with("stored feedback is unreadable")
	);
	assert!(
		FeedbackError::Store(sqlx::Error::PoolClosed)
			.to_string()
			.starts_with("feedback storage failed")
	);
}

#[test]
fn viewer_ids_are_stable_per_project_and_unlinkable_across_projects() {
	let first = viewer_id("secret", "browser-token", 1);
	assert_eq!(first, viewer_id("secret", "browser-token", 1));
	assert!(first.starts_with("v-"));
	assert_eq!(first.len(), 22);
	assert_ne!(first, viewer_id("secret", "browser-token", 2));
	assert_ne!(first, viewer_id("other-secret", "browser-token", 1));
	assert_ne!(first, viewer_id("secret", "another-browser", 1));
}

#[tokio::test]
async fn public_projects_resolve_without_a_session() {
	let state = state().await;
	assert!(
		public_project(&state, "nobody", "invoices")
			.await
			.unwrap()
			.is_none()
	);
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', '')")
		.execute(&state.db)
		.await
		.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type, organization_id) VALUES (1, 1, 1001, 'acme', 'Organization', 1)")
		.execute(&state.db)
		.await
		.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'acme/api', 0)")
		.execute(&state.db)
		.await
		.unwrap();
	sqlx::query("INSERT INTO project_repositories (project_id, repository_external_id, full_name) VALUES (1, 11, 'acme/api'), (1, 12, 'acme/gone')")
		.execute(&state.db)
		.await
		.unwrap();

	let found = public_project(&state, "ACME", "invoices")
		.await
		.unwrap()
		.unwrap();
	assert_eq!(found.organization, "acme");
	assert_eq!(found.project.name, "Invoices");
	assert_eq!(found.scope.repositories.len(), 1);
	assert_eq!(found.scope.repositories[0].github_installation_id, 1001);
	assert_eq!(found.scope.disconnected, 1);
	assert_eq!(found.scope.visibility(), RepositoryVisibility::Private);
	assert!(
		public_project(&state, "acme", "missing")
			.await
			.unwrap()
			.is_none()
	);
}
