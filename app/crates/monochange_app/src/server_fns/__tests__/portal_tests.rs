//! Anonymous visitors share, vote, answer, and follow their feedback.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use axum::http::header::LOCATION;
use axum::http::header::SET_COOKIE;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use leptos_axum::ResponseOptions;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::Stage;

use super::VIEWER_COOKIE;
use super::portal_path;
use super::portal_reply;
use super::portal_view;
use super::portal_vote;
use super::preview_markdown;
use super::share_feedback;
use super::similar_requests;
use crate::server_fns::feedback::feedback_action;
use crate::tests::feedback::context;
use crate::tests::feedback::state;
use crate::tests::feedback::submit;

fn location(response: &ResponseOptions) -> Option<String> {
	response
		.0
		.read()
		.unwrap()
		.headers
		.get(LOCATION)
		.map(|value| value.to_str().unwrap().to_owned())
}

fn message(error: &server_fn::ServerFnError) -> String {
	crate::pages::organization::action_error(error)
}

fn args(project: &str) -> (String, String) {
	("alice".to_owned(), project.to_owned())
}

#[tokio::test]
async fn first_visits_get_a_viewer_cookie_and_unknown_projects_nothing() {
	let state = state().await;
	let (owner, response) = context(&state, None, None);
	let (organization, project) = args("pocketbook");
	let snapshot = owner
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(snapshot.project_name, "Pocketbook");
	assert_eq!(snapshot.description, "Invoicing");
	assert_eq!(snapshot.visibility, RepositoryVisibility::Private);
	let cookie = response.0.read().unwrap().headers[SET_COOKIE]
		.to_str()
		.unwrap()
		.to_owned();
	assert!(cookie.starts_with(&format!("{VIEWER_COOKIE}=")));
	assert!(cookie.contains("HttpOnly") && cookie.contains("Secure") && cookie.contains("Path=/"));

	// A returning visitor keeps their token.
	let (owner, response) = context(&state, None, Some("returning"));
	let (organization, project) = args("pocketbook");
	owner
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap();
	assert!(response.0.read().unwrap().headers.get(SET_COOKIE).is_none());

	let (organization, project) = args("missing");
	assert!(
		owner
			.with(|| ScopedFuture::new(portal_view(organization, project)))
			.await
			.unwrap()
			.is_none()
	);
	assert_eq!(portal_path("alice", "pocketbook"), "/p/alice/pocketbook");
}

#[tokio::test]
async fn sharing_files_an_item_the_visitor_can_follow() {
	let state = state().await;
	let (owner, response) = context(&state, None, Some("token-a"));
	let share = |kind: &str, description: &str, project: &str| {
		let (organization, project) = args(project);
		let (kind, description) = (kind.to_owned(), description.to_owned());
		owner.with(|| ScopedFuture::new(share_feedback(organization, project, kind, description)))
	};

	share(
		"feature_request",
		"**Dark** mode for invoices",
		"pocketbook",
	)
	.await
	.unwrap();
	assert_eq!(
		location(&response).as_deref(),
		Some("/p/alice/pocketbook?shared=fb-1#yours")
	);
	assert_eq!(
		message(&share("praise", "Nice", "pocketbook").await.unwrap_err()),
		"Choose a bug or an idea."
	);
	assert_eq!(
		message(&share("bug_report", "  ", "pocketbook").await.unwrap_err()),
		"The description is empty."
	);
	assert_eq!(
		message(&share("bug_report", "Hi", "missing").await.unwrap_err()),
		"That project doesn't exist."
	);

	let (organization, project) = args("pocketbook");
	let snapshot = owner
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(snapshot.own.len(), 1);
	let own = &snapshot.own[0];
	assert_eq!(own.id, "fb-1");
	assert_eq!(own.stage, Stage::Discussing);
	assert_eq!(own.update.title, "Dark mode for invoices");
	assert!(own.question.as_deref().unwrap().contains("What outcome"));
	assert!(snapshot.threads.contains_key("fb-1"));

	// Someone else sees the request on the roadmap but not as theirs.
	let (other, _) = context(&state, None, Some("token-b"));
	let (organization, project) = args("pocketbook");
	let theirs = other
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap()
		.unwrap();
	assert!(theirs.own.is_empty());
	assert_eq!(theirs.feed.roadmap.len(), 1);
}

#[tokio::test]
async fn answering_the_question_opens_voting_and_updates_follow() {
	let state = state().await;
	let (owner, response) = context(&state, None, Some("token-a"));
	let (organization, project) = args("pocketbook");
	owner
		.with(|| {
			ScopedFuture::new(share_feedback(
				organization,
				project,
				"feature_request".to_owned(),
				"Dark mode for invoices".to_owned(),
			))
		})
		.await
		.unwrap();
	let (organization, project) = args("pocketbook");
	owner
		.with(|| {
			ScopedFuture::new(portal_reply(
				organization,
				project,
				"fb-1".to_owned(),
				"Follow the system theme".to_owned(),
			))
		})
		.await
		.unwrap();
	assert_eq!(
		location(&response).as_deref(),
		Some("/p/alice/pocketbook#item-fb-1")
	);

	// Another visitor votes, then changes their mind, then votes again.
	let (voter, vote_response) = context(&state, None, Some("token-b"));
	for on in [true, false, true] {
		let (organization, project) = args("pocketbook");
		voter
			.with(|| ScopedFuture::new(portal_vote(organization, project, "fb-1".to_owned(), on)))
			.await
			.unwrap();
	}
	assert_eq!(
		location(&vote_response).as_deref(),
		Some("/p/alice/pocketbook#roadmap")
	);
	let (organization, project) = args("pocketbook");
	let duplicate = voter
		.with(|| ScopedFuture::new(portal_vote(organization, project, "fb-1".to_owned(), true)))
		.await
		.unwrap_err();
	assert!(message(&duplicate).ends_with("has already voted."));

	// The maintainer accepts; both subscribers hear about it.
	let (maintainer, _) = context(&state, Some(1), None);
	maintainer
		.with(|| {
			ScopedFuture::new(feedback_action(
				"alice".to_owned(),
				"pocketbook".to_owned(),
				"fb-1".to_owned(),
				"accept".to_owned(),
				None,
				Some("Popular".to_owned()),
				None,
				None,
				None,
			))
		})
		.await
		.unwrap();
	for viewer in [&owner, &voter] {
		let (organization, project) = args("pocketbook");
		let snapshot = viewer
			.with(|| ScopedFuture::new(portal_view(organization, project)))
			.await
			.unwrap()
			.unwrap();
		assert_eq!(snapshot.updates[0].update.body, "Accepted and planned");
	}
	let (organization, project) = args("pocketbook");
	let voted_items = voter
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap()
		.unwrap()
		.voted;
	assert_eq!(voted_items, ["fb-1"]);
}

#[tokio::test]
async fn similar_requests_are_offered_before_filing() {
	let state = state().await;
	submit(
		&state,
		"v-1",
		FeedbackKind::FeatureRequest,
		"Dark mode for invoices",
	)
	.await;
	let (owner, _) = context(&state, None, Some("token-a"));
	let (organization, project) = args("pocketbook");
	let matches = owner
		.with(|| {
			ScopedFuture::new(similar_requests(
				organization,
				project,
				"dark mode invoices".to_owned(),
			))
		})
		.await
		.unwrap();
	assert_eq!(matches[0].id, "fb-1");
	let (organization, project) = args("missing");
	assert!(
		owner
			.with(|| ScopedFuture::new(similar_requests(organization, project, "dark".to_owned())))
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn previews_render_markdown_within_the_size_limit() {
	let html = preview_markdown("**Hi**".to_owned()).await.unwrap();
	assert_eq!(html.trim(), "<p><strong>Hi</strong></p>");
	let long = "x".repeat(monochange_app_feedback::submission::MAX_DESCRIPTION_CHARS + 1);
	assert_eq!(
		message(&preview_markdown(long).await.unwrap_err()),
		"That's longer than feedback can be."
	);
}

#[tokio::test]
async fn storage_failures_are_reported_politely() {
	let state = state().await;
	sqlx::query("DROP TABLE project_feedback")
		.execute(&state.db)
		.await
		.unwrap();
	let (owner, _) = context(&state, None, Some("token-a"));
	let (organization, project) = args("pocketbook");
	let error = owner
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap_err();
	assert_eq!(
		message(&error),
		"Feedback couldn't be saved. Please try again."
	);
}
