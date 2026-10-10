//! The public feedback portal for a project: share a bug or idea, vote, answer
//! follow-up questions, and follow what happens.
//!
//! Visitors are pseudonymous. A random viewer token in a cookie becomes a
//! per-project id ([`monochange_app_api::feedback::viewer_id`]), so votes are
//! deduplicated without accounts and ids can't be linked across projects.

#[cfg(test)]
#[path = "__tests__/portal_tests.rs"]
mod tests;

use std::collections::BTreeMap;

use leptos::server;
use monochange_app_feedback::Notification;
use monochange_app_feedback::OutboundUpdate;
use monochange_app_feedback::PublicMessage;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::SimilarItem;
use monochange_app_feedback::Stage;
use monochange_app_feedback::StatusFeed;
use serde::Deserialize;
use serde::Serialize;

/// The cookie that holds a visitor's random viewer token.
pub const VIEWER_COOKIE: &str = "__Host-monochange_viewer";

/// How many updates the "yours" section shows.
#[cfg(not(target_arch = "wasm32"))]
const UPDATE_LIMIT: u32 = 20;

/// One of the visitor's own submissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnItem {
	pub id: String,
	pub stage: Stage,
	pub update: OutboundUpdate,
	/// The assistant's open question, while the item waits for an answer.
	pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortalSnapshot {
	pub organization: String,
	pub project_slug: String,
	pub project_name: String,
	pub description: String,
	pub visibility: RepositoryVisibility,
	pub feed: StatusFeed,
	/// Items the visitor has voted for.
	pub voted: Vec<String>,
	/// Public discussion per listed item, redacted for the portal.
	pub threads: BTreeMap<String, Vec<PublicMessage>>,
	/// Newest first.
	pub own: Vec<OwnItem>,
	/// Newest first.
	pub updates: Vec<Notification>,
}

pub fn portal_path(organization: &str, project: &str) -> String {
	format!("/p/{organization}/{project}")
}

#[cfg(not(target_arch = "wasm32"))]
type ServerResult<T> = Result<T, server_fn::ServerFnError>;

/// The visitor's viewer token, minting one (and setting its cookie) on first
/// visit.
#[cfg(not(target_arch = "wasm32"))]
async fn viewer_token() -> ServerResult<String> {
	use axum_extra::extract::cookie::Cookie;
	use axum_extra::extract::cookie::CookieJar;
	use axum_extra::extract::cookie::SameSite;
	use leptos_axum::ResponseOptions;
	use leptos_axum::extract;

	let jar: CookieJar = extract().await?;
	if let Some(token) = jar
		.get(VIEWER_COOKIE)
		.map(|cookie| cookie.value().to_owned())
		.filter(|token| !token.is_empty())
	{
		return Ok(token);
	}
	let token = uuid::Uuid::new_v4().to_string();
	let cookie = Cookie::build((VIEWER_COOKIE, token.clone()))
		.path("/")
		.secure(true)
		.http_only(true)
		.same_site(SameSite::Lax)
		.max_age(time::Duration::days(365))
		.build();
	let response: ResponseOptions = leptos::prelude::expect_context();
	response.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);
	Ok(token)
}

#[cfg(not(target_arch = "wasm32"))]
use super::feedback::feedback_error;

/// The project, its scope, and the visitor's id in it.
#[cfg(not(target_arch = "wasm32"))]
async fn visitor_context(
	state: &monochange_app_api::AppState,
	organization: &str,
	project: &str,
) -> ServerResult<Option<(monochange_app_api::feedback::PublicProject, String)>> {
	let Some(public) = monochange_app_api::feedback::public_project(state, organization, project)
		.await
		.map_err(feedback_error)?
	else {
		return Ok(None);
	};
	let token = viewer_token().await?;
	let viewer =
		monochange_app_api::feedback::viewer_id(&state.jwt_secret, &token, public.project.id);
	Ok(Some((public, viewer)))
}

#[cfg(not(target_arch = "wasm32"))]
fn not_found() -> server_fn::ServerFnError {
	server_fn::ServerFnError::new("That project doesn't exist.")
}

/// Sends a plain form post back to the portal, at `anchor`.
#[cfg(not(target_arch = "wasm32"))]
fn back_to_portal(public: &monochange_app_api::feedback::PublicProject, query: &str, anchor: &str) {
	leptos_axum::redirect(&format!(
		"{}{query}#{anchor}",
		portal_path(&public.organization, &public.project.slug)
	));
}

/// Everything the portal shows the current visitor, or `None` for an
/// unknown project.
#[server]
pub async fn portal_view(
	organization: String,
	project: String,
) -> Result<Option<PortalSnapshot>, server_fn::ServerFnError> {
	use std::sync::Arc;

	use monochange_app_feedback::Actor;
	use monochange_app_feedback::roadmap;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let Some((public, viewer)) = visitor_context(&state, &organization, &project).await? else {
		return Ok(None);
	};
	let feedback = monochange_app_api::feedback::load(&state, &public.scope)
		.await
		.map_err(feedback_error)?;
	let service = &feedback.service;
	let mut threads = BTreeMap::new();
	let mut voted = Vec::new();
	let mut own = Vec::new();
	for item in service.items() {
		let mine = item.submission.submitter.anonymous_id == viewer;
		if roadmap::is_listed(item) || mine {
			threads.insert(
				item.id.clone(),
				service
					.public_thread(&item.id)
					.map_err(|error| server_fn::ServerFnError::new(error.to_string()))?,
			);
		}
		if item.votes.has_voted(&viewer) {
			voted.push(item.id.clone());
		}
		if mine {
			let question = (item.stage == Stage::Discussing)
				.then(|| {
					item.visible_discussion()
						.into_iter()
						.rev()
						.find(|message| message.author == Actor::Ai)
						.map(|message| message.body.clone())
				})
				.flatten();
			own.push(OwnItem {
				id: item.id.clone(),
				stage: item.stage,
				update: service
					.public_item_update(&item.id)
					.map_err(|error| server_fn::ServerFnError::new(error.to_string()))?,
				question,
			});
		}
	}
	own.reverse();
	let updates = monochange_app_api::feedback::notifications_for(
		&state,
		&public.scope,
		&viewer,
		UPDATE_LIMIT,
	)
	.await
	.map_err(feedback_error)?;
	Ok(Some(PortalSnapshot {
		organization: public.organization.clone(),
		project_slug: public.project.slug.clone(),
		project_name: public.project.name.clone(),
		description: public.project.description.clone(),
		visibility: service.policy().visibility,
		feed: service.status_feed().feed,
		voted,
		threads,
		own,
		updates,
	}))
}

/// Shares a bug report or feature request through the portal.
#[server]
pub async fn share_feedback(
	organization: String,
	project: String,
	kind: String,
	description: String,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	use monochange_app_feedback::FeedbackKind;
	use monochange_app_feedback::FeedbackSubmission;
	use monochange_app_feedback::SubmitterIdentity;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let (public, viewer) = visitor_context(&state, &organization, &project)
		.await?
		.ok_or_else(not_found)?;
	let kind = match kind.as_str() {
		"bug_report" => FeedbackKind::BugReport,
		"feature_request" => FeedbackKind::FeatureRequest,
		_ => return Err(server_fn::ServerFnError::new("Choose a bug or an idea.")),
	};
	let submission = FeedbackSubmission {
		kind,
		description,
		page: None,
		attachments: Vec::new(),
		submitter: SubmitterIdentity {
			anonymous_id: viewer,
			email: None,
		},
		app_slug: monochange_app_api::feedback::PORTAL_APP.to_owned(),
	};
	let receipt = monochange_app_api::feedback::update(&state, &public.scope, |service| {
		service.receive(submission.clone())
	})
	.await
	.map_err(feedback_error)?;
	back_to_portal(&public, &format!("?shared={}", receipt.id), "yours");
	Ok(())
}

/// Adds or removes the visitor's vote.
#[server]
pub async fn portal_vote(
	organization: String,
	project: String,
	item: String,
	on: bool,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let (public, viewer) = visitor_context(&state, &organization, &project)
		.await?
		.ok_or_else(not_found)?;
	monochange_app_api::feedback::update(&state, &public.scope, |service| {
		if on {
			service.vote(&item, &viewer)
		} else {
			service.retract_vote(&item, &viewer)
		}
		.map(|_| ())
	})
	.await
	.map_err(feedback_error)?;
	back_to_portal(&public, "", "roadmap");
	Ok(())
}

/// Adds the visitor's message to an item's discussion.
#[server]
pub async fn portal_reply(
	organization: String,
	project: String,
	item: String,
	body: String,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	use monochange_app_feedback::Actor;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let (public, viewer) = visitor_context(&state, &organization, &project)
		.await?
		.ok_or_else(not_found)?;
	let author = Actor::User(viewer);
	monochange_app_api::feedback::update(&state, &public.scope, |service| {
		service.reply(&item, &author, &body, Vec::new()).map(|_| ())
	})
	.await
	.map_err(feedback_error)?;
	back_to_portal(&public, "", &format!("item-{item}"));
	Ok(())
}

/// Listed requests that look like `text`, so visitors can vote instead of
/// filing a duplicate.
#[server]
pub async fn similar_requests(
	organization: String,
	project: String,
	text: String,
) -> Result<Vec<SimilarItem>, server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let Some(public) =
		monochange_app_api::feedback::public_project(&state, &organization, &project)
			.await
			.map_err(feedback_error)?
	else {
		return Ok(Vec::new());
	};
	let feedback = monochange_app_api::feedback::load(&state, &public.scope)
		.await
		.map_err(feedback_error)?;
	Ok(feedback.service.similar(&text, 3))
}

/// Renders the editor's preview. Anyone may call it, so input is capped at
/// the longest description the pipeline accepts.
// Leptos server functions must be async.
#[allow(clippy::unused_async)]
#[server]
pub async fn preview_markdown(text: String) -> Result<String, server_fn::ServerFnError> {
	if text.chars().count() > monochange_app_feedback::submission::MAX_DESCRIPTION_CHARS {
		return Err(server_fn::ServerFnError::new(
			"That's longer than feedback can be.",
		));
	}
	Ok(crate::markdown::render(&text))
}
