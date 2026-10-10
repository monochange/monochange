//! A project's feedback, loaded from and saved to the database.
//!
//! Every change goes through [`update`]: load the project's document, apply
//! one operation to a [`FeedbackService`], and save it with its notifications
//! in one transaction. A concurrent writer makes the save conflict; the
//! operation then reruns against the fresh document, so operations passed to
//! [`update`] must not have side effects outside the service.
//!
//! Calls to `GitHub` happen between updates, never inside one. Creating an
//! issue first records the decision, then calls `GitHub`, then links the
//! issue, so a failed call leaves an accepted item that can be retried.

use monochange_app_db::feedback::NotificationRecord;
use monochange_app_db::feedback::SaveOutcome;
use monochange_app_feedback::CadenceSnapshot;
use monochange_app_feedback::DisclosurePolicy;
use monochange_app_feedback::FeedbackService;
use monochange_app_feedback::FeedbackState;
use monochange_app_feedback::IssueRef;
use monochange_app_feedback::PullRequestRef;
use monochange_app_feedback::RegisteredApp;
use monochange_app_feedback::ReleaseLink;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::RuleBasedTriage;
use monochange_app_feedback::ServiceError;
use thiserror::Error;

use crate::AppState;
use crate::github_app::GitHubAppError;

/// Slug of the app every project has: its own public portal.
pub const PORTAL_APP: &str = "portal";

/// How many times an update reruns after losing a race.
const UPDATE_ATTEMPTS: usize = 3;

#[derive(Debug, Error)]
pub enum FeedbackError {
	#[error("feedback storage failed: {0}")]
	Store(#[from] sqlx::Error),
	#[error("stored feedback is unreadable: {0}")]
	Corrupt(#[from] serde_json::Error),
	#[error(transparent)]
	Service(#[from] ServiceError),
	#[error("feedback is changing quickly right now; try again")]
	Busy,
	#[error("the GitHub App isn't configured on this deployment")]
	GitHubAppMissing,
	#[error("{0} isn't one of this project's connected repositories")]
	UnknownRepository(String),
	#[error(transparent)]
	GitHub(#[from] GitHubAppError),
}

/// A connected repository in a project, as feedback needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRepository {
	pub full_name: String,
	pub private: bool,
	pub github_installation_id: i64,
}

/// Which project's feedback to act on, resolved and authorised by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScope {
	pub project_id: i32,
	/// The project's connected repositories.
	pub repositories: Vec<ProjectRepository>,
	/// Repositories still in the project but no longer connected. Their
	/// visibility can't be checked, so they count as private.
	pub disconnected: usize,
}

impl ProjectScope {
	/// Private repositories make the whole project private: one feed covers
	/// all of them, so it follows the strictest repository. A disconnected
	/// repository might be private, so it counts as one.
	pub fn visibility(&self) -> RepositoryVisibility {
		if self.disconnected > 0
			|| self
				.repositories
				.iter()
				.any(|repository| repository.private)
		{
			RepositoryVisibility::Private
		} else {
			RepositoryVisibility::Public
		}
	}

	fn repository(&self, full_name: &str) -> Result<&ProjectRepository, FeedbackError> {
		self.repositories
			.iter()
			.find(|repository| repository.full_name.eq_ignore_ascii_case(full_name))
			.ok_or_else(|| FeedbackError::UnknownRepository(full_name.to_owned()))
	}
}

/// A loaded project document and the version it was read at.
pub struct ProjectFeedback {
	pub service: FeedbackService<RuleBasedTriage>,
	version: Option<i64>,
}

fn portal_app() -> RegisteredApp {
	RegisteredApp {
		slug: PORTAL_APP.to_owned(),
		display_name: "Project portal".to_owned(),
		allowed_origins: Vec::new(),
	}
}

/// Loads a project's feedback. A project without a document starts empty with
/// its portal registered.
pub async fn load(
	state: &AppState,
	scope: &ProjectScope,
) -> Result<ProjectFeedback, FeedbackError> {
	let stored = monochange_app_db::feedback::load_feedback(&state.db, scope.project_id).await?;
	let (document, version) = match stored {
		Some(stored) => {
			(
				serde_json::from_str::<FeedbackState>(&stored.state_json)?,
				Some(stored.version),
			)
		}
		None => (FeedbackState::default(), None),
	};
	let mut service = FeedbackService::from_state(
		RuleBasedTriage,
		DisclosurePolicy::for_visibility(scope.visibility()),
		CadenceSnapshot::default(),
		document,
	);
	if service.app(PORTAL_APP).is_none() {
		service.register_app(portal_app());
	}
	Ok(ProjectFeedback { service, version })
}

/// Applies `operation` to the project's feedback and saves the result with
/// its notifications, rerunning on a lost race. An operation that changes
/// nothing saves nothing.
pub async fn update<R>(
	state: &AppState,
	scope: &ProjectScope,
	mut operation: impl FnMut(&mut FeedbackService<RuleBasedTriage>) -> Result<R, ServiceError>,
) -> Result<R, FeedbackError> {
	for _ in 0..UPDATE_ATTEMPTS {
		let mut feedback = load(state, scope).await?;
		let before = serde_json::to_string(&feedback.service.state())?;
		let result = operation(&mut feedback.service)?;
		let notifications = feedback
			.service
			.drain_notifications()
			.into_iter()
			.map(|notification| {
				Ok(NotificationRecord {
					recipient: notification.recipient.clone(),
					item_id: notification.item_id.clone(),
					notification_json: serde_json::to_string(&notification)?,
				})
			})
			.collect::<Result<Vec<_>, serde_json::Error>>()?;
		let state_json = serde_json::to_string(&feedback.service.state())?;
		if state_json == before && notifications.is_empty() {
			return Ok(result);
		}
		match monochange_app_db::feedback::save_feedback(
			&state.db,
			scope.project_id,
			&state_json,
			feedback.version,
			&notifications,
		)
		.await?
		{
			SaveOutcome::Saved { .. } => return Ok(result),
			SaveOutcome::Conflict => {}
		}
	}
	Err(FeedbackError::Busy)
}

/// Opens the `GitHub` issue for an accepted item in `repository` and links
/// it. Returns the linked issue.
pub async fn create_issue(
	state: &AppState,
	scope: &ProjectScope,
	item_id: &str,
	repository: &str,
) -> Result<IssueRef, FeedbackError> {
	let target = scope.repository(repository)?;
	let app = state
		.github_app
		.as_ref()
		.ok_or(FeedbackError::GitHubAppMissing)?;
	// The issue lives in one repository, so it is redacted for that
	// repository's readers rather than the project's strictest policy.
	let mut feedback = load(state, scope).await?;
	feedback
		.service
		.set_policy(DisclosurePolicy::for_visibility(if target.private {
			RepositoryVisibility::Private
		} else {
			RepositoryVisibility::Public
		}));
	let draft = feedback.service.issue_draft(item_id)?;
	let token = app
		.installation_token(&state.http, target.github_installation_id)
		.await?;
	let created = crate::github_app::create_issue(
		&state.http,
		&app.api_url,
		&token,
		&target.full_name,
		&draft.title,
		&draft.body,
		&draft.labels,
	)
	.await?;
	let issue = IssueRef {
		repository: Some(target.full_name.clone()),
		number: created.number,
		url: Some(created.html_url),
	};
	let linked = issue.clone();
	update(state, scope, move |service| {
		service.link_issue(item_id, linked.clone())
	})
	.await?;
	Ok(issue)
}

/// A project as anyone may see it, resolved without a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicProject {
	pub organization: String,
	pub project: monochange_app_db::projects::ProjectRecord,
	pub scope: ProjectScope,
}

/// Resolves `organization/project` for the public portal. Only connected
/// repositories join the scope; disconnected ones make it private.
pub async fn public_project(
	state: &AppState,
	organization: &str,
	project: &str,
) -> Result<Option<PublicProject>, FeedbackError> {
	let Some(record) = monochange_app_db::projects::find_organization(
		&state.db,
		monochange_app_db::projects::GITHUB,
		organization,
	)
	.await?
	else {
		return Ok(None);
	};
	let Some(found) =
		monochange_app_db::projects::find_project(&state.db, record.id, project).await?
	else {
		return Ok(None);
	};
	Ok(Some(PublicProject {
		organization: record.login,
		scope: project_scope(state, found.id, found.repository_count).await?,
		project: found,
	}))
}

/// The scope of project `project_id`, which links `linked` repositories.
/// Only connected repositories join it; the rest count as disconnected.
async fn project_scope(
	state: &AppState,
	project_id: i32,
	linked: i64,
) -> Result<ProjectScope, FeedbackError> {
	use sqlx::Row;

	let rows = sqlx::query(
		"SELECT r.github_full_name, r.github_private, i.github_installation_id
		 FROM project_repositories p
		 JOIN repositories r ON r.github_repo_id = p.repository_external_id AND r.provider = p.provider
		 JOIN installations i ON i.id = r.installation_id
		 WHERE p.project_id = $1
		 ORDER BY r.github_full_name COLLATE NOCASE",
	)
	.bind(project_id)
	.fetch_all(&state.db)
	.await?;
	let repositories: Vec<ProjectRepository> = rows
		.iter()
		.map(|row| {
			ProjectRepository {
				full_name: row.get("github_full_name"),
				private: row.get("github_private"),
				github_installation_id: row.get("github_installation_id"),
			}
		})
		.collect();
	let disconnected = usize::try_from(linked)
		.unwrap_or(usize::MAX)
		.saturating_sub(repositories.len());
	Ok(ProjectScope {
		project_id,
		repositories,
		disconnected,
	})
}

/// Something that happened in a repository on the way from an accepted item
/// to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
	/// A pull request was opened or its description changed.
	PullRequestOpened {
		pull_request: PullRequestRef,
		body: String,
	},
	PullRequestMerged {
		repository: String,
		number: u64,
	},
	ReleasePublished {
		repository: String,
		release: ReleaseLink,
	},
}

/// Applies `delivery` in GitHub repository `github_repo_id` to every project
/// that includes it. Returns the ids of the items it advanced, per project.
pub async fn observe_delivery(
	state: &AppState,
	github_repo_id: i64,
	delivery: &Delivery,
) -> Result<Vec<(i32, Vec<String>)>, FeedbackError> {
	use sqlx::Row;

	let projects = sqlx::query(
		"SELECT p.project_id,
		        (SELECT COUNT(*) FROM project_repositories c WHERE c.project_id = p.project_id) AS linked
		 FROM project_repositories p
		 WHERE p.provider = $1 AND p.repository_external_id = $2
		 ORDER BY p.project_id",
	)
	.bind(monochange_app_db::projects::GITHUB)
	.bind(github_repo_id)
	.fetch_all(&state.db)
	.await?;
	let mut advanced = Vec::new();
	for row in projects {
		let scope = project_scope(state, row.get("project_id"), row.get("linked")).await?;
		let ids = update(state, &scope, |service| {
			match delivery {
				Delivery::PullRequestOpened { pull_request, body } => {
					service.observe_pull_request(pull_request, body)
				}
				Delivery::PullRequestMerged { repository, number } => {
					service
						.observe_merge(Some(repository), *number)
						.map(|id| id.into_iter().collect())
				}
				Delivery::ReleasePublished {
					repository,
					release,
				} => service.ship_merged(Some(repository), release),
			}
		})
		.await?;
		if !ids.is_empty() {
			advanced.push((scope.project_id, ids));
		}
	}
	Ok(advanced)
}

/// A visitor's pseudonymous id within one project, derived from their
/// browser's random viewer token. Keyed by the server secret and the project,
/// so ids can't be forged or linked across projects.
pub fn viewer_id(secret: &str, viewer_token: &str, project_id: i32) -> String {
	use hmac::Hmac;
	use hmac::Mac;
	use sha2::Sha256;

	let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
		.unwrap_or_else(|error| panic!("HMAC accepts keys of any length: {error}"));
	mac.update(format!("{project_id}:{viewer_token}").as_bytes());
	let digest = hex::encode(mac.finalize().into_bytes());
	format!("v-{}", &digest[..20])
}

/// Notifications delivered to one subscriber, newest first.
pub async fn notifications_for(
	state: &AppState,
	scope: &ProjectScope,
	recipient: &str,
	limit: u32,
) -> Result<Vec<monochange_app_feedback::Notification>, FeedbackError> {
	monochange_app_db::feedback::list_notifications(&state.db, scope.project_id, recipient, limit)
		.await?
		.iter()
		.map(|json| Ok(serde_json::from_str(json)?))
		.collect()
}

#[cfg(test)]
#[path = "__tests__/feedback_tests.rs"]
mod tests;
