//! The public task API lists only open tasks, and needs no sign-in.

// `#[tokio::test]` generates `Builder::expect("Failed building the Runtime")`.
#![allow(clippy::disallowed_methods)]

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::FeedbackSubmission;
use monochange_app_feedback::IssueRef;
use monochange_app_feedback::MaintainerDecision;
use monochange_app_feedback::SubmitterIdentity;
use monochange_app_feedback::TaskDraft;
use tower::ServiceExt;

use super::TaskList;
use crate::AppSecrets;
use crate::AppState;
use crate::api_router;
use crate::feedback::PORTAL_APP;
use crate::feedback::public_project;
use crate::feedback::update;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

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
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "tasks-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	AppState::new(db, secrets).unwrap()
}

fn draft(check: &str) -> TaskDraft {
	TaskDraft {
		title: "Round quarterly totals to cents".to_owned(),
		acceptance: vec!["Quarterly totals match their invoices".to_owned()],
		paths: vec!["src/totals/**".to_owned()],
		check: check.to_owned(),
	}
}

/// Shares three tasks on one accepted item: one listed, one rejected, and
/// one already expired.
async fn shared(state: &AppState) {
	let scope = public_project(state, "acme", "invoices")
		.await
		.unwrap()
		.unwrap()
		.scope;
	let now = chrono::Utc::now().timestamp();
	update(state, &scope, |service| {
		service.set_shared_compute(true);
		service.receive(FeedbackSubmission {
			kind: FeedbackKind::BugReport,
			description: "Quarterly totals are off by a cent".to_owned(),
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
		})?;
		service.accept(
			"fb-1",
			MaintainerDecision {
				maintainer: "ifiok".to_owned(),
				rationale: "Fits".to_owned(),
				override_vote_threshold: Some("Important".to_owned()),
			},
		)?;
		service.link_issue(
			"fb-1",
			IssueRef {
				repository: Some("acme/invoices".to_owned()),
				number: 42,
				url: None,
			},
		)?;
		let share = |service: &mut monochange_app_feedback::FeedbackService<_>,
		             check: &str,
		             expires_at: i64| {
			service.share_task("fb-1", COMMIT, &draft(check), "ifiok", now, expires_at)
		};
		share(service, "cargo test totals", now + 3600)?;
		share(service, "printenv", now + 3600)?;
		share(service, "cargo test totals", now - 1)?;
		Ok(())
	})
	.await
	.unwrap();
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, serde_json::Value) {
	let response = api_router(state.clone())
		.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
		.await
		.unwrap();
	let status = response.status();
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	(status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn anyone_can_read_a_projects_open_tasks() {
	let state = state().await;
	let (status, body) = get(&state, "/api/projects/acme/invoices/tasks").await;
	assert_eq!(status, StatusCode::OK);
	assert_eq!(
		serde_json::from_value::<TaskList>(body).unwrap(),
		TaskList {
			organization: "acme".to_owned(),
			project: "invoices".to_owned(),
			tasks: Vec::new(),
		}
	);

	shared(&state).await;
	let (status, body) = get(&state, "/api/projects/acme/invoices/tasks").await;
	assert_eq!(status, StatusCode::OK);
	let list: TaskList = serde_json::from_value(body).unwrap();
	assert_eq!(
		list.tasks
			.iter()
			.map(|task| task.id.as_str())
			.collect::<Vec<_>>(),
		["task-1"]
	);
	// What the CLI hands an agent is exactly what was screened.
	assert_eq!(list.tasks[0].check, "cargo test totals");
	assert_eq!(list.tasks[0].commit, COMMIT);
	assert_eq!(list.tasks[0].issue_number, 42);

	let (status, body) = get(&state, "/api/projects/acme/invoices/tasks/task-1").await;
	assert_eq!(status, StatusCode::OK);
	assert_eq!(
		serde_json::from_value::<monochange_app_feedback::PublicTask>(body).unwrap(),
		list.tasks[0]
	);
}

#[tokio::test]
async fn unlisted_tasks_and_unknown_projects_are_not_found() {
	let state = state().await;
	shared(&state).await;

	for (uri, message) in [
		(
			"/api/projects/acme/invoices/tasks/task-2",
			"no open task with that id; it may have expired or been withdrawn",
		),
		(
			"/api/projects/acme/invoices/tasks/task-3",
			"no open task with that id; it may have expired or been withdrawn",
		),
		("/api/projects/acme/missing/tasks", "no such project"),
		(
			"/api/projects/nobody/invoices/tasks/task-1",
			"no such project",
		),
	] {
		let (status, body) = get(&state, uri).await;
		assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
		assert_eq!(body, serde_json::json!({ "message": message }), "{uri}");
	}
}

#[tokio::test]
async fn unreadable_feedback_is_reported_without_details() {
	let state = state().await;
	monochange_app_db::feedback::save_feedback(&state.db, 1, "not json", None, &[])
		.await
		.unwrap();

	let (status, body) = get(&state, "/api/projects/acme/invoices/tasks").await;
	assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
	assert_eq!(
		body,
		serde_json::json!({ "message": "tasks can't be read right now" })
	);
}
