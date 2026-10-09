//! The maintainer console for a project's feedback.

#[cfg(test)]
#[path = "__tests__/feedback_tests.rs"]
mod tests;

use leptos::server;
use monochange_app_feedback::FeedbackItem;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::SimilarItem;
use monochange_app_feedback::Stage;
use serde::Deserialize;
use serde::Serialize;

/// What a maintainer can do to an item right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleAction {
	Resume,
	OpenVoting,
	Accept,
	Decline,
	Duplicate,
	EditSummary,
	CreateIssue,
	Close,
}

impl ConsoleAction {
	pub const ALL: [ConsoleAction; 8] = [
		ConsoleAction::Resume,
		ConsoleAction::OpenVoting,
		ConsoleAction::Accept,
		ConsoleAction::Decline,
		ConsoleAction::Duplicate,
		ConsoleAction::EditSummary,
		ConsoleAction::CreateIssue,
		ConsoleAction::Close,
	];

	pub fn parse(name: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|action| action.as_str() == name)
	}

	pub fn as_str(self) -> &'static str {
		match self {
			ConsoleAction::Resume => "resume",
			ConsoleAction::OpenVoting => "open_voting",
			ConsoleAction::Accept => "accept",
			ConsoleAction::Decline => "decline",
			ConsoleAction::Duplicate => "duplicate",
			ConsoleAction::EditSummary => "edit_summary",
			ConsoleAction::CreateIssue => "create_issue",
			ConsoleAction::Close => "close",
		}
	}
}

/// The actions that make sense for `item`.
pub fn console_actions(item: &FeedbackItem) -> Vec<ConsoleAction> {
	match item.stage {
		Stage::Quarantined => {
			vec![
				ConsoleAction::Resume,
				ConsoleAction::Decline,
				ConsoleAction::Close,
			]
		}
		Stage::Discussing => {
			vec![
				ConsoleAction::OpenVoting,
				ConsoleAction::Accept,
				ConsoleAction::Decline,
				ConsoleAction::Duplicate,
				ConsoleAction::EditSummary,
				ConsoleAction::Close,
			]
		}
		Stage::Voting => {
			vec![
				ConsoleAction::Accept,
				ConsoleAction::Decline,
				ConsoleAction::Duplicate,
				ConsoleAction::EditSummary,
				ConsoleAction::Close,
			]
		}
		Stage::Accepted if item.issue.is_none() => {
			vec![
				ConsoleAction::CreateIssue,
				ConsoleAction::EditSummary,
				ConsoleAction::Close,
			]
		}
		Stage::Accepted | Stage::Building | Stage::InReview => {
			vec![ConsoleAction::EditSummary, ConsoleAction::Close]
		}
		_ => Vec::new(),
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsoleItem {
	pub item: FeedbackItem,
	/// The title users see, or `None` while the item isn't listed.
	pub portal_title: Option<String>,
	/// What the portal redaction removed from that title.
	pub redacted: Vec<String>,
	/// Why the gate refused to publish this item, when it did.
	pub withheld: Option<String>,
	pub actions: Vec<ConsoleAction>,
	/// Listed items that look like the same request.
	pub similar: Vec<SimilarItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsoleView {
	pub organization: String,
	pub project_name: String,
	pub project_slug: String,
	pub visibility: RepositoryVisibility,
	pub acceptance_threshold: u32,
	/// Connected repositories an issue can be opened in.
	pub repositories: Vec<String>,
	/// Newest first.
	pub items: Vec<ConsoleItem>,
}

/// Turns a feedback failure into a message for the person who caused it.
/// Storage and `GitHub` failures are logged and summarised; the rest are
/// already user-facing.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn feedback_error(
	error: monochange_app_api::feedback::FeedbackError,
) -> server_fn::ServerFnError {
	use monochange_app_api::feedback::FeedbackError;

	match error {
		FeedbackError::Store(_) | FeedbackError::Corrupt(_) | FeedbackError::GitHub(_) => {
			tracing::warn!(%error, "feedback operation failed");
			server_fn::ServerFnError::new(match error {
				FeedbackError::GitHub(_) => {
					"GitHub didn't accept the request. Check that the GitHub App can write issues in that repository, then try again."
				}
				_ => "Feedback couldn't be saved. Please try again.",
			})
		}
		FeedbackError::Busy => {
			server_fn::ServerFnError::new(
				"Feedback is changing quickly right now. Please try again.",
			)
		}
		FeedbackError::GitHubAppMissing => {
			server_fn::ServerFnError::new(
				"The GitHub App isn't configured on this deployment, so issues can't be created yet.",
			)
		}
		FeedbackError::UnknownRepository(repository) => {
			server_fn::ServerFnError::new(format!("{repository} isn't connected to this project."))
		}
		FeedbackError::Service(error) => {
			server_fn::ServerFnError::new(sentence(&error.to_string()))
		}
	}
}

/// Capitalizes a library error message so it reads as a sentence.
#[cfg(not(target_arch = "wasm32"))]
fn sentence(message: &str) -> String {
	let mut characters = message.chars();
	match characters.next() {
		Some(first) => {
			format!(
				"{}{}.",
				first.to_uppercase(),
				characters.as_str().trim_end_matches('.')
			)
		}
		None => String::new(),
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn not_found() -> server_fn::ServerFnError {
	server_fn::ServerFnError::new("That project isn't available to you.")
}

#[cfg(not(target_arch = "wasm32"))]
fn optional(value: Option<String>) -> Option<String> {
	value
		.map(|value| value.trim().to_owned())
		.filter(|value| !value.is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn console_view(
	maintained: &super::organizations::MaintainedProject,
	service: &monochange_app_feedback::FeedbackService<monochange_app_feedback::RuleBasedTriage>,
) -> ConsoleView {
	use monochange_app_feedback::Surface;
	use monochange_app_feedback::disclosure::redact;
	use monochange_app_feedback::roadmap;

	let visibility = service.policy().visibility;
	let feed = service.status_feed();
	let items = service
		.items()
		.iter()
		.rev()
		.map(|item| {
			let withheld = feed
				.withheld
				.iter()
				.find(|withheld: &&monochange_app_feedback::Withheld| withheld.id == item.id)
				.map(|withheld| withheld.reason.clone());
			let redaction = roadmap::public_title(item)
				.filter(|_| roadmap::is_listed(item))
				.map(|title| redact(&title, Surface::Portal, visibility));
			ConsoleItem {
				portal_title: redaction
					.as_ref()
					.filter(|_| withheld.is_none())
					.map(|redaction| redaction.text.clone()),
				redacted: redaction.map_or_else(Vec::new, |redaction| {
					redaction
						.removed
						.into_iter()
						.map(|removed| removed.label().to_owned())
						.collect()
				}),
				withheld,
				actions: console_actions(item),
				similar: service
					.similar(&item.submission.description, 4)
					.into_iter()
					.filter(|similar| similar.id != item.id)
					.collect(),
				item: item.clone(),
			}
		})
		.collect();
	ConsoleView {
		organization: maintained.organization.clone(),
		project_name: maintained.project.name.clone(),
		project_slug: maintained.project.slug.clone(),
		visibility,
		acceptance_threshold: service.state().rules.acceptance_threshold,
		repositories: maintained
			.scope
			.repositories
			.iter()
			.map(|repository| repository.full_name.clone())
			.collect(),
		items,
	}
}

/// The project's feedback for its maintainer, or `None` when it isn't theirs.
#[server]
pub async fn feedback_console(
	organization: String,
	project: String,
) -> Result<Option<ConsoleView>, server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let Some(maintained) =
		super::organizations::maintained_project(&state, &organization, &project).await?
	else {
		return Ok(None);
	};
	let feedback = monochange_app_api::feedback::load(&state, &maintained.scope)
		.await
		.map_err(feedback_error)?;
	Ok(Some(console_view(&maintained, &feedback.service)))
}

/// One maintainer decision on an item. Fields not used by `action` are
/// ignored; empty fields count as missing.
#[server]
#[allow(clippy::too_many_arguments)]
pub async fn feedback_action(
	organization: String,
	project: String,
	item: String,
	action: String,
	#[server(default)] rationale: Option<String>,
	#[server(default)] override_rationale: Option<String>,
	#[server(default)] duplicate_of: Option<String>,
	#[server(default)] summary: Option<String>,
	#[server(default)] repository: Option<String>,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	use monochange_app_feedback::MaintainerDecision;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let project_access = super::organizations::maintained_project(&state, &organization, &project)
		.await?
		.ok_or_else(not_found)?;
	let maintainer = project_access.maintainer.clone();
	let decision = MaintainerDecision {
		maintainer: maintainer.clone(),
		rationale: optional(rationale).unwrap_or_else(|| "Decided by a maintainer".to_owned()),
		override_vote_threshold: optional(override_rationale),
	};
	let scope = &project_access.scope;
	let action = ConsoleAction::parse(&action)
		.ok_or_else(|| server_fn::ServerFnError::new(format!("Unknown action {action}.")))?;
	let required = |value: Option<String>, message: &str| {
		optional(value).ok_or_else(|| server_fn::ServerFnError::new(message.to_owned()))
	};
	let result = match action {
		ConsoleAction::CreateIssue => {
			let repository = required(repository, "Choose the repository the issue belongs in.")?;
			monochange_app_api::feedback::create_issue(&state, scope, &item, &repository)
				.await
				.map(|_| ())
		}
		ConsoleAction::Duplicate => {
			let canonical = required(duplicate_of, "Choose the request this one duplicates.")?;
			monochange_app_api::feedback::update(&state, scope, |service| {
				service
					.mark_duplicate(&item, &canonical, &maintainer)
					.map(|_| ())
			})
			.await
		}
		ConsoleAction::EditSummary => {
			let summary = required(summary, "Write the title users should see.")?;
			monochange_app_api::feedback::update(&state, scope, |service| {
				service
					.edit_summary(&item, &maintainer, &summary)
					.map(|_| ())
			})
			.await
		}
		decision_action => {
			monochange_app_api::feedback::update(&state, scope, |service| {
				match decision_action {
					ConsoleAction::Resume => service.resume_triage(&item, &maintainer),
					ConsoleAction::OpenVoting => service.open_voting(&item, &maintainer),
					ConsoleAction::Accept => service.accept(&item, decision.clone()),
					ConsoleAction::Decline => service.decline(&item, decision.clone()),
					_ => service.close(&item, &maintainer),
				}
				.map(|_| ())
			})
			.await
		}
	};
	result.map_err(feedback_error)
}

/// Posts a maintainer reply to an item's discussion.
#[server]
pub async fn maintainer_reply(
	organization: String,
	project: String,
	item: String,
	body: String,
) -> Result<(), server_fn::ServerFnError> {
	use std::sync::Arc;

	use monochange_app_feedback::Actor;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let maintained = super::organizations::maintained_project(&state, &organization, &project)
		.await?
		.ok_or_else(not_found)?;
	let author = Actor::Maintainer(maintained.maintainer.clone());
	monochange_app_api::feedback::update(&state, &maintained.scope, |service| {
		service.reply(&item, &author, &body, Vec::new()).map(|_| ())
	})
	.await
	.map_err(feedback_error)
}
