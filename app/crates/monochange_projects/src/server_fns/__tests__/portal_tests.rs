//! Anyone reads a portal; GitHub accounts share, vote, answer, and follow
//! their feedback.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::http::Request;
use axum::http::header::COOKIE;
use axum::http::header::LOCATION;
use axum::http::header::SET_COOKIE;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use leptos_axum::ResponseOptions;
use monochange_app_api::AppState;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::Stage;

use super::navigation_sign_in_path;
use super::portal_path;
use super::portal_reply;
use super::portal_sign_in_path;
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

/// A request carrying `cookie` verbatim, such as a forged session.
fn context_with_cookie(state: &Arc<AppState>, cookie: &str) -> Owner {
	let owner = Owner::new();
	let mut request = Request::new(());
	request
		.headers_mut()
		.insert(COOKIE, cookie.parse().unwrap());
	let (parts, ()) = request.into_parts();
	owner.with(|| {
		provide_context(state.clone());
		provide_context(parts);
		provide_context(ResponseOptions::default());
	});
	owner
}

#[tokio::test]
async fn anyone_reads_a_portal_without_an_account() {
	let state = state().await;
	submit(
		&state,
		"v-1",
		FeedbackKind::FeatureRequest,
		"Dark mode for invoices",
	)
	.await;
	let (owner, response) = context(&state, None);
	let (organization, project) = args("pocketbook");
	let snapshot = owner
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(snapshot.project_name, "Pocketbook");
	assert_eq!(snapshot.description, "Invoicing");
	assert_eq!(snapshot.visibility, RepositoryVisibility::Private);
	assert!(snapshot.viewer.is_none());
	assert!(snapshot.voted.is_empty() && snapshot.own.is_empty() && snapshot.updates.is_empty());

	// Reading sets no cookie: there is nothing to remember about a reader.
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
	assert_eq!(
		portal_sign_in_path("alice", "pocketbook", "roadmap"),
		"/login?next=%2Fp%2Falice%2Fpocketbook%23roadmap"
	);
	assert_eq!(
		navigation_sign_in_path("/p/alice/pocketbook"),
		"/login?next=%2Fp%2Falice%2Fpocketbook"
	);
	assert_eq!(navigation_sign_in_path("/install"), "/login");
	assert_eq!(navigation_sign_in_path("/"), "/login");
}

#[tokio::test]
async fn writing_needs_a_github_account_and_a_forged_session_reads_as_signed_out() {
	let state = state().await;
	let id = submit(
		&state,
		"v-1",
		FeedbackKind::FeatureRequest,
		"Dark mode for invoices",
	)
	.await;
	let forged = context_with_cookie(
		&state,
		&format!("{}=forged", monochange_app_api::oauth::SESSION_COOKIE_NAME),
	);
	let (signed_out, _) = context(&state, None);

	for owner in [&signed_out, &forged] {
		let (organization, project) = args("pocketbook");
		let snapshot = owner
			.with(|| ScopedFuture::new(portal_view(organization, project)))
			.await
			.unwrap()
			.unwrap();
		assert!(snapshot.viewer.is_none());

		let (organization, project) = args("pocketbook");
		let shared = owner
			.with(|| {
				ScopedFuture::new(share_feedback(
					organization,
					project,
					"bug_report".to_owned(),
					"Totals are wrong".to_owned(),
				))
			})
			.await
			.unwrap_err();
		assert_eq!(message(&shared), "Sign in with GitHub to share.");
		let (organization, project) = args("pocketbook");
		let voted = owner
			.with(|| ScopedFuture::new(portal_vote(organization, project, id.clone(), true)))
			.await
			.unwrap_err();
		assert_eq!(message(&voted), "Sign in with GitHub to vote.");
		let (organization, project) = args("pocketbook");
		let replied = owner
			.with(|| {
				ScopedFuture::new(portal_reply(
					organization,
					project,
					id.clone(),
					"Me too".to_owned(),
				))
			})
			.await
			.unwrap_err();
		assert_eq!(
			message(&replied),
			"Sign in with GitHub to join the discussion."
		);
	}

	// An unknown project is still reported as missing first.
	let (organization, project) = args("missing");
	let missing = signed_out
		.with(|| ScopedFuture::new(portal_vote(organization, project, id.clone(), true)))
		.await
		.unwrap_err();
	assert_eq!(message(&missing), "That project doesn't exist.");

	let public = monochange_app_api::feedback::public_project(&state, "alice", "pocketbook")
		.await
		.unwrap()
		.unwrap();
	let feedback = monochange_app_api::feedback::load(&state, &public.scope)
		.await
		.unwrap();
	assert_eq!(feedback.service.items().len(), 1);
	assert_eq!(feedback.service.items()[0].votes.total(), 0);
}

#[tokio::test]
async fn one_account_is_one_pseudonymous_voter_per_project() {
	let state = state().await;
	let id = submit(
		&state,
		"v-1",
		FeedbackKind::FeatureRequest,
		"Dark mode for invoices",
	)
	.await;
	let public = monochange_app_api::feedback::public_project(&state, "alice", "pocketbook")
		.await
		.unwrap()
		.unwrap();
	let voter = monochange_app_api::feedback::account_viewer_id(
		&state.jwt_secret,
		"github",
		303,
		public.project.id,
	);

	// Carol votes from two browsers; the second is the same account.
	for _ in 0..2 {
		let (carol, _) = context(&state, Some(3));
		let (organization, project) = args("pocketbook");
		let _ = carol
			.with(|| ScopedFuture::new(portal_vote(organization, project, id.clone(), true)))
			.await;
	}

	let feedback = monochange_app_api::feedback::load(&state, &public.scope)
		.await
		.unwrap();
	let item = &feedback.service.items()[0];
	assert_eq!(item.votes.total(), 1);
	assert!(item.votes.has_voted(&voter));
	assert!(!voter.contains("303") && !voter.contains("carol"));

	let (carol, _) = context(&state, Some(3));
	let (organization, project) = args("pocketbook");
	let snapshot = carol
		.with(|| ScopedFuture::new(portal_view(organization, project)))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(snapshot.voted, [id]);
	let viewer = snapshot.viewer.unwrap();
	assert_eq!(viewer.login, "carol");
	assert_eq!(viewer.avatar_url, None);
}

#[tokio::test]
async fn sharing_files_an_item_the_visitor_can_follow() {
	let state = state().await;
	let (owner, response) = context(&state, Some(3));
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
	let (other, _) = context(&state, Some(4));
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
	let (owner, response) = context(&state, Some(3));
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
	let (voter, vote_response) = context(&state, Some(4));

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
	let (maintainer, _) = context(&state, Some(1));
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
	let (owner, _) = context(&state, Some(3));
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
	let (owner, _) = context(&state, Some(3));
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
