//! The public feedback portal for a project: share a bug or idea, vote, answer
//! follow-up questions, and follow what happens.
//!
//! Anyone can read a portal. Sharing, voting, and discussing need a GitHub
//! sign-in, because votes decide what gets built and a browser cookie can be
//! minted again by clearing it. The account becomes a per-project id
//! ([`monochange_app_api::feedback::account_viewer_id`]), so stored feedback
//! never names a GitHub login and ids can't be linked across projects.

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

/// The signed-in visitor, as only they see themselves on the portal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortalViewer {
	pub login: String,
	pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortalSnapshot {
	pub organization: String,
	pub project_slug: String,
	pub project_name: String,
	pub description: String,
	pub visibility: RepositoryVisibility,
	pub feed: StatusFeed,
	/// `None` until the visitor signs in with GitHub.
	pub viewer: Option<PortalViewer>,
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

/// Signs in with GitHub and comes back to the portal at `anchor`.
pub fn portal_sign_in_path(organization: &str, project: &str, anchor: &str) -> String {
	sign_in_returning(&format!("{}#{anchor}", portal_path(organization, project)))
}

/// The navigation's sign-in link from `path`: a portal visitor comes back to
/// the portal, and everyone else goes on to the dashboard.
pub fn navigation_sign_in_path(path: &str) -> String {
	if path.starts_with("/p/") {
		sign_in_returning(path)
	} else {
		"/login".to_owned()
	}
}

fn sign_in_returning(next: &str) -> String {
	format!(
		"/login?next={}",
		url::form_urlencoded::byte_serialize(next.as_bytes()).collect::<String>()
	)
}

#[cfg(not(target_arch = "wasm32"))]
type ServerResult<T> = Result<T, server_fn::ServerFnError>;

#[cfg(not(target_arch = "wasm32"))]
use super::feedback::feedback_error;

/// The signed-in GitHub account. A missing, expired, or forged session reads
/// as signed out, so a stale cookie never hides a public portal.
#[cfg(not(target_arch = "wasm32"))]
async fn signed_in_visitor() -> Option<super::auth::SessionUser> {
	super::auth::get_session().await.ok().flatten()
}

/// The project, and the visitor's account and id in it when signed in.
#[cfg(not(target_arch = "wasm32"))]
async fn visitor_context(
	state: &monochange_app_api::AppState,
	organization: &str,
	project: &str,
) -> ServerResult<
	Option<(
		monochange_app_api::feedback::PublicProject,
		Option<(super::auth::SessionUser, String)>,
	)>,
> {
	let Some(public) = monochange_app_api::feedback::public_project(state, organization, project)
		.await
		.map_err(feedback_error)?
	else {
		return Ok(None);
	};
	let visitor = signed_in_visitor().await.map(|user| {
		let viewer = monochange_app_api::feedback::account_viewer_id(
			&state.jwt_secret,
			monochange_app_db::projects::GITHUB,
			user.github_id,
			public.project.id,
		);
		(user, viewer)
	});
	Ok(Some((public, visitor)))
}

/// The project and the signed-in visitor's id in it, for anything that writes.
#[cfg(not(target_arch = "wasm32"))]
async fn signed_in_context(
	state: &monochange_app_api::AppState,
	organization: &str,
	project: &str,
	doing: &str,
) -> ServerResult<(monochange_app_api::feedback::PublicProject, String)> {
	let (public, visitor) = visitor_context(state, organization, project)
		.await?
		.ok_or_else(not_found)?;
	let (_, viewer) = visitor
		.ok_or_else(|| server_fn::ServerFnError::new(format!("Sign in with GitHub to {doing}.")))?;
	Ok((public, viewer))
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
	let Some((public, visitor)) = visitor_context(&state, &organization, &project).await? else {
		return Ok(None);
	};
	let viewer = visitor.as_ref().map(|(_, viewer)| viewer.as_str());
	let feedback = monochange_app_api::feedback::load(&state, &public.scope)
		.await
		.map_err(feedback_error)?;
	let service = &feedback.service;
	let mut threads = BTreeMap::new();
	let mut voted = Vec::new();
	let mut own = Vec::new();

	for item in service.items() {
		let mine = viewer == Some(item.submission.submitter.anonymous_id.as_str());

		if roadmap::is_listed(item) || mine {
			threads.insert(
				item.id.clone(),
				service
					.public_thread(&item.id)
					.map_err(|error| server_fn::ServerFnError::new(error.to_string()))?,
			);
		}

		if viewer.is_some_and(|viewer| item.votes.has_voted(viewer)) {
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
	let updates = match viewer {
		Some(viewer) => {
			monochange_app_api::feedback::notifications_for(
				&state,
				&public.scope,
				viewer,
				UPDATE_LIMIT,
			)
			.await
			.map_err(feedback_error)?
		}
		None => Vec::new(),
	};
	Ok(Some(PortalSnapshot {
		organization: public.organization.clone(),
		project_slug: public.project.slug.clone(),
		project_name: public.project.name.clone(),
		description: public.project.description.clone(),
		visibility: service.policy().visibility,
		feed: service.status_feed().feed,
		viewer: visitor.map(|(user, _)| {
			PortalViewer {
				login: user.github_login,
				avatar_url: user.github_avatar_url,
			}
		}),
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
	let (public, viewer) = signed_in_context(&state, &organization, &project, "share").await?;

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
	let (public, viewer) = signed_in_context(&state, &organization, &project, "vote").await?;
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
	let (public, viewer) =
		signed_in_context(&state, &organization, &project, "join the discussion").await?;
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
