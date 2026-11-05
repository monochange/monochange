//! Maintainer-authorized operations. Callers resolve project ownership first.

use monochange_app_feedback::FeedbackService;
use monochange_app_feedback::RuleBasedTriage;
use monochange_app_feedback::ServiceError;
use monochange_app_feedback::Stage;
use monochange_app_feedback::embedded::ClientSurface;
use monochange_app_feedback::embedded::EmbeddedError;
use monochange_app_feedback::embedded::Integration;
use monochange_app_feedback::embedded::WorkerPairing;
use monochange_app_feedback::planning::PlanningBrief;
use monochange_app_feedback::planning::PlanningError;
use monochange_app_feedback::planning::ReviewDecision;
use serde::Deserialize;
use serde::Serialize;

use super::ApiFailure;
use super::credential;
use super::fingerprint;
use super::now;
use crate::AppState;
use crate::feedback::ProjectScope;
use crate::feedback::{self};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationInput {
	pub name: String,
	pub namespace: String,
	pub origins: Vec<String>,
	pub surfaces: Vec<ClientSurface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedCredential {
	pub id: String,
	pub token: String,
	pub project: i32,
	pub expires_at: Option<i64>,
}

/// Origin normalization happens once at registration, never by suffix matching.
pub fn origins(values: &[String]) -> Result<Vec<String>, EmbeddedError> {
	values
		.iter()
		.map(|value| {
			let parsed =
				url::Url::parse(value).map_err(|_| EmbeddedError::Invalid("web origin"))?;
			if !parsed.username().is_empty()
				|| parsed.password().is_some()
				|| parsed.query().is_some()
				|| parsed.fragment().is_some()
				|| parsed.path() != "/"
				|| !(parsed.scheme() == "https"
					|| parsed.scheme() == "http"
						&& matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
			{
				return Err(EmbeddedError::Invalid(
					"web origins require HTTPS, or localhost for development",
				));
			}
			Ok(parsed.origin().ascii_serialization())
		})
		.collect()
}

pub async fn register(
	state: &AppState,
	scope: &ProjectScope,
	input: IntegrationInput,
) -> Result<IssuedCredential, ApiFailure> {
	let allowed = origins(&input.origins)?;

	if input.surfaces.contains(&ClientSurface::Web) && allowed.is_empty() {
		return Err(EmbeddedError::Invalid("at least one web origin is required").into());
	}

	let id = uuid::Uuid::new_v4().to_string();
	let token = credential("mcfi", scope.project_id);
	let salt = credential("identity", scope.project_id);
	let integration = Integration {
		id: id.clone(),
		name: input.name,
		namespace: input.namespace,
		origins: allowed,
		surfaces: input.surfaces,
		credential_hash: fingerprint(&token),
		generation: 0,
		revoked: false,
	};
	feedback::update(state, scope, |service| {
		service
			.embedded_mut()
			.register(integration.clone(), &salt)?;
		Ok(())
	})
	.await?;
	Ok(IssuedCredential {
		id,
		token,
		project: scope.project_id,
		expires_at: None,
	})
}

pub async fn rotate(
	state: &AppState,
	scope: &ProjectScope,
	id: &str,
	revoke: bool,
) -> Result<IssuedCredential, ApiFailure> {
	let token = credential("mcfi", scope.project_id);
	feedback::update(state, scope, |service| {
		service
			.embedded_mut()
			.rotate(id, &fingerprint(&token), revoke)?;
		Ok(())
	})
	.await?;
	Ok(IssuedCredential {
		id: id.into(),
		token: if revoke { String::new() } else { token },
		project: scope.project_id,
		expires_at: None,
	})
}

pub async fn pairing(
	state: &AppState,
	scope: &ProjectScope,
	name: &str,
	repositories: &[String],
) -> Result<IssuedCredential, ApiFailure> {
	if name.trim().is_empty()
		|| name.len() > 120
		|| repositories.is_empty()
		|| repositories.iter().any(|name| {
			!scope
				.repositories
				.iter()
				.any(|repository| &repository.full_name == name)
		}) {
		return Err(EmbeddedError::Invalid("worker name or repositories").into());
	}
	let token = credential("mcfp", scope.project_id);
	let expires_at = now() + 600;
	feedback::update(state, scope, |service| {
		service
			.embedded_mut()
			.pairings
			.retain(|pairing| pairing.expires_at > now());
		if service.embedded().pairings.len() >= 16 {
			return Err(EmbeddedError::RateLimited.into());
		}

		service.embedded_mut().pairings.push(WorkerPairing {
			code_hash: fingerprint(&token),
			name: name.into(),
			repositories: repositories.to_vec(),
			expires_at,
		});
		Ok(())
	})
	.await?;
	Ok(IssuedCredential {
		id: String::new(),
		token,
		project: scope.project_id,
		expires_at: Some(expires_at),
	})
}

pub async fn revoke_worker(
	state: &AppState,
	scope: &ProjectScope,
	id: &str,
) -> Result<(), ApiFailure> {
	feedback::update(state, scope, |service| {
		let worker = service
			.embedded_mut()
			.workers
			.iter_mut()
			.find(|worker| worker.id == id)
			.ok_or(EmbeddedError::NotFound)?;
		worker.revoked = true;
		for job in &mut service.planning_mut().jobs {
			if job.worker.as_deref() == Some(id)
				&& matches!(
					job.status,
					monochange_app_feedback::planning::JobStatus::Running
				) {
				job.status = monochange_app_feedback::planning::JobStatus::CancelRequested;
			}
		}

		Ok(())
	})
	.await
	.map_err(Into::into)
}

/// Hash material user content, not votes or the request's delivery stage.
pub fn brief_version(
	service: &FeedbackService<RuleBasedTriage>,
	item: &str,
	repository: &str,
	commit: &str,
) -> Result<String, ServiceError> {
	let item = service
		.item(item)
		.ok_or_else(|| ServiceError::NotFound(item.into()))?;
	let revision = service
		.planning()
		.jobs
		.iter()
		.rev()
		.find(|job| job.brief.item == item.id && job.review == Some(ReviewDecision::RequestChanges))
		.and_then(|job| job.reason.as_deref());
	let content = serde_json::json!({"description":item.submission.description,"page":item.submission.page,"discussion":item.visible_discussion(),"summary":item.summary_override,"repository":repository,"commit":commit,"revision":revision});
	Ok(fingerprint(&content.to_string()))
}

pub async fn head(
	state: &AppState,
	scope: &ProjectScope,
	repository: &str,
) -> Result<String, ApiFailure> {
	let target = scope
		.repositories
		.iter()
		.find(|target| target.full_name == repository)
		.ok_or_else(|| ApiFailure::invalid("repository is not connected to this project"))?;
	let app = state
		.github_app
		.as_ref()
		.ok_or_else(|| ApiFailure::invalid("connect the GitHub App before planning"))?;
	let token = app
		.installation_token(&state.http, target.github_installation_id)
		.await
		.map_err(feedback::FeedbackError::from)?;
	crate::github_app::default_branch_head(&state.http, &app.api_url, &token, repository)
		.await
		.map_err(feedback::FeedbackError::from)
		.map_err(Into::into)
}

fn queue(
	service: &mut FeedbackService<RuleBasedTriage>,
	item_id: &str,
	repository: &str,
	commit: &str,
	automatic: bool,
	time: i64,
) -> Result<String, ServiceError> {
	let item = service
		.item(item_id)
		.ok_or_else(|| ServiceError::NotFound(item_id.into()))?;

	if !matches!(item.stage, Stage::Voting | Stage::Accepted) || item.duplicate_of.is_some() {
		return Err(
			PlanningError::Invalid("the request must finish triage before planning").into(),
		);
	}

	let version = brief_version(service, item_id, repository, commit)?;
	let revision = service
		.planning()
		.jobs
		.iter()
		.rev()
		.find(|job| job.brief.item == item_id && job.review == Some(ReviewDecision::RequestChanges))
		.and_then(|job| job.reason.as_deref());
	let report = serde_json::json!({"report":item.submission.description,"context":item.submission.page,"discussion":item.visible_discussion(),"requested_changes":revision});
	let text = monochange_app_feedback::disclosure::redact(
		&report.to_string(),
		monochange_app_feedback::Surface::Repository,
		service.policy().visibility,
	)
	.text;
	service
		.planning_mut()
		.queue(
			PlanningBrief {
				item: item_id.into(),
				version,
				repository: repository.into(),
				commit: commit.into(),
				text,
			},
			automatic,
			time,
		)
		.map_err(Into::into)
}

pub async fn queue_plan(
	state: &AppState,
	scope: &ProjectScope,
	item: &str,
	repository: &str,
) -> Result<String, ApiFailure> {
	let commit = head(state, scope, repository).await?;
	feedback::update(state, scope, |service| {
		queue(service, item, repository, &commit, false, now())
	})
	.await
	.map_err(Into::into)
}

pub async fn set_policy(
	state: &AppState,
	scope: &ProjectScope,
	enabled: bool,
	repository: &str,
	daily_limit: usize,
	actor: &str,
) -> Result<(), ApiFailure> {
	if !(1..=100).contains(&daily_limit) {
		return Err(ApiFailure::invalid(
			"daily planning limit must be between 1 and 100",
		));
	}

	if enabled
		&& !scope
			.repositories
			.iter()
			.any(|target| target.full_name == repository)
	{
		return Err(ApiFailure::invalid(
			"select a connected planning repository",
		));
	}
	feedback::update(state, scope, |service| {
		let count = service.items().len();
		service
			.planning_mut()
			.set_policy(enabled, repository, count, actor, now())?;
		service.planning_mut().policy.daily_limit = daily_limit;
		Ok(())
	})
	.await
	.map_err(Into::into)
}

/// Called when a worker asks for work; failures leave intake safely persisted.
pub async fn schedule_automatic(state: &AppState, scope: &ProjectScope) -> Result<(), ApiFailure> {
	let loaded = feedback::load(state, scope).await?;
	let policy = loaded.service.planning().policy.clone();

	if !policy.automatic_features {
		return Ok(());
	}

	let candidates: Vec<String> = loaded
		.service
		.items()
		.iter()
		.skip(policy.automatic_after)
		.filter(|item| {
			matches!(item.stage, Stage::Voting | Stage::Accepted)
				&& item.duplicate_of.is_none()
				&& item.triage.as_ref().is_some_and(|report| {
					report.classification == monochange_app_feedback::FeedbackKind::FeatureRequest
				}) && loaded
				.service
				.planning()
				.jobs
				.iter()
				.rev()
				.find(|job| {
					job.kind == monochange_app_feedback::planning::JobKind::Plan
						&& job.brief.item == item.id
				})
				.is_none_or(|job| {
					matches!(
						job.status,
						monochange_app_feedback::planning::JobStatus::AwaitingApproval
							| monochange_app_feedback::planning::JobStatus::Superseded
					) && brief_version(
						&loaded.service,
						&item.id,
						&job.brief.repository,
						&job.brief.commit,
					)
					.ok()
					.as_deref() != Some(&job.brief.version)
				})
		})
		.map(|item| item.id.clone())
		.collect();

	if candidates.is_empty() {
		return Ok(());
	}

	let commit = head(state, scope, &policy.repository).await?;

	for item in candidates {
		feedback::update(state, scope, |service| {
			if !service.planning().policy.automatic_features
				|| service.planning().policy.repository != policy.repository
				|| service.planning().policy.automatic_after != policy.automatic_after
			{
				return Ok(());
			}
			match queue(service, &item, &policy.repository, &commit, true, now()) {
				Err(ServiceError::Planning(PlanningError::Budget)) => Ok(()),
				result => result.map(|_| ()),
			}
		})
		.await?;
	}

	Ok(())
}

pub async fn approve(
	state: &AppState,
	scope: &ProjectScope,
	job: &str,
	actor: &str,
) -> Result<String, ApiFailure> {
	let loaded = feedback::load(state, scope).await?;
	let plan = loaded
		.service
		.planning()
		.job(job)
		.map_err(|error| ApiFailure::invalid(error.to_string()))?
		.clone();
	let commit = head(state, scope, &plan.brief.repository).await?;

	if commit != plan.brief.commit {
		return Err(ApiFailure::invalid(
			"the repository changed; generate a fresh plan",
		));
	}

	feedback::update(state, scope, |service| {
		let version = brief_version(service, &plan.brief.item, &plan.brief.repository, &commit)?;
		if service
			.item(&plan.brief.item)
			.is_none_or(|item| item.stage != Stage::Accepted || item.issue.is_none())
		{
			return Err(PlanningError::Invalid(
				"accept the request and create its issue before implementation",
			)
			.into());
		}
		let id = service
			.planning_mut()
			.approve(job, &version, actor, now())?;
		Ok(id)
	})
	.await
	.map_err(Into::into)
}

pub async fn review(
	state: &AppState,
	scope: &ProjectScope,
	job: &str,
	decision: ReviewDecision,
	actor: &str,
	reason: &str,
) -> Result<(), ApiFailure> {
	feedback::update(state, scope, |service| {
		service
			.planning_mut()
			.review(job, decision, actor, reason, now())?;
		Ok(())
	})
	.await
	.map_err(Into::into)
}

/// Intake persists durable scheduling intent in the request; workers retry this scan on reconnect.
pub(super) fn schedule_after_write(state: &AppState, scope: &ProjectScope) {
	let state = state.clone();
	let scope = scope.clone();
	tokio::spawn(async move {
		if let Err(error) = schedule_automatic(&state, &scope).await {
			tracing::warn!(
				code = error.code,
				"automatic planning is waiting for repository access"
			);
		}
	});
}
