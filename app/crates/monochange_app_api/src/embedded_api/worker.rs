//! Workers connect outward and may only return results for their assigned lease.

use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use monochange_app_feedback::PullRequestRef;
use monochange_app_feedback::Stage;
use monochange_app_feedback::embedded::EmbeddedError;
use monochange_app_feedback::embedded::Worker;
use monochange_app_feedback::planning::ImplementationPlan;
use monochange_app_feedback::planning::JobKind;
use monochange_app_feedback::planning::JobStatus;
use monochange_app_feedback::planning::PlanningError;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use super::ApiFailure;
use super::ApiResult;
use super::Parameters;
use super::admin;
use super::auth;
use super::credential;
use super::fingerprint;
use super::now;
use crate::AppState;
use crate::feedback;
use crate::github_app::feedback::FeedbackFile;
use crate::github_app::feedback::FeedbackPullRequest;
use crate::github_app::feedback::create_feedback_pull_request;

pub(super) async fn pair(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
) -> ApiResult<Value> {
	if headers.contains_key("origin") {
		return Err(EmbeddedError::Unauthorized.into());
	}

	let scope = auth::scope(&state, path.project).await?;
	let digest = auth::bearer(&headers, "mcfp", path.project)?;
	let token = credential("mcfw", path.project);
	let id = uuid::Uuid::new_v4().to_string();
	feedback::update(&state, &scope, |service| {
		let pairing = service
			.embedded()
			.pairings
			.iter()
			.find(|pairing| pairing.code_hash == digest && pairing.expires_at > now())
			.ok_or(EmbeddedError::Unauthorized)?
			.clone();
		service.embedded_mut().pair(
			&digest,
			Worker {
				id: id.clone(),
				name: pairing.name,
				credential_hash: fingerprint(&token),
				repositories: pairing.repositories,
				revoked: false,
				last_seen: now(),
			},
			now(),
		)?;
		Ok(())
	})
	.await?;
	Ok(Json(json!({"id":id,"token":token,"project":path.project})))
}

pub(super) async fn claim(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
) -> ApiResult<Value> {
	let scope = auth::scope(&state, path.project).await?;
	let digest = auth::bearer(&headers, "mcfw", path.project)?;
	feedback::load(&state, &scope)
		.await?
		.service
		.embedded()
		.worker(&digest)?;

	if let Err(error) = admin::schedule_automatic(&state, &scope).await {
		tracing::warn!(
			code = error.code,
			"automatic feedback planning is waiting for repository access"
		);
	}

	let lease = credential("lease", path.project);
	let job = feedback::update(&state, &scope, |service| {
		let worker = service.embedded().worker(&digest)?.clone();
		service
			.embedded_mut()
			.quota(&format!("worker:{}:claim", worker.id), 720, 3600, now())?;
		let obsolete: Vec<String> = service
			.planning()
			.jobs
			.iter()
			.filter(|job| job.status == JobStatus::Queued)
			.filter_map(|job| {
				let version = admin::brief_version(
					service,
					&job.brief.item,
					&job.brief.repository,
					&job.brief.commit,
				)
				.ok();
				(version.as_deref() != Some(&job.brief.version)
					|| !scope
						.repositories
						.iter()
						.any(|repository| repository.full_name == job.brief.repository))
				.then(|| job.id.clone())
			})
			.collect();
		for job in &mut service.planning_mut().jobs {
			if obsolete.contains(&job.id) {
				job.status = JobStatus::Superseded;
			}
		}

		let job = service.planning_mut().claim(
			&worker.id,
			&fingerprint(&lease),
			&worker.repositories,
			now(),
		);
		if let Some(worker) = service
			.embedded_mut()
			.workers
			.iter_mut()
			.find(|record| record.id == worker.id)
		{
			worker.last_seen = now();
		}
		Ok(job)
	})
	.await?;
	Ok(Json(json!({"job":job,"lease":job.as_ref().map(|_| lease)})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Heartbeat {
	lease: String,
}

pub(super) async fn heartbeat(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<Heartbeat>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;
	let scope = auth::scope(&state, path.project).await?;
	let digest = auth::bearer(&headers, "mcfw", path.project)?;
	let stop = feedback::update(&state, &scope, |service| {
		let worker = service.embedded().worker(&digest)?.clone();
		let stop = service.planning_mut().heartbeat(
			path.id.as_deref().ok_or(EmbeddedError::NotFound)?,
			&worker.id,
			&fingerprint(&input.lease),
			now(),
		)?;
		if let Some(record) = service
			.embedded_mut()
			.workers
			.iter_mut()
			.find(|record| record.id == worker.id)
		{
			record.last_seen = now();
		}
		Ok(stop)
	})
	.await?;
	Ok(Json(json!({"cancel":stop,"lease_until":now()+120})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerResult {
	lease: String,
	plan: Option<ImplementationPlan>,
	changes: Option<Vec<FeedbackFile>>,
	failure: Option<String>,
	cancelled: Option<bool>,
}

pub(super) async fn result(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<WorkerResult>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;
	let scope = auth::scope(&state, path.project).await?;
	let digest = auth::bearer(&headers, "mcfw", path.project)?;
	let id = path.id()?;
	let loaded = feedback::load(&state, &scope).await?;
	let worker = loaded.service.embedded().worker(&digest)?.clone();
	let job = loaded
		.service
		.planning()
		.job(id)
		.map_err(|error| ApiFailure::invalid(error.to_string()))?
		.clone();
	let lease = fingerprint(&input.lease);

	if job.worker.as_deref() != Some(&worker.id) || job.lease.as_deref() != Some(&lease) {
		return Err(EmbeddedError::Unauthorized.into());
	}

	if input.failure.is_some() || input.cancelled == Some(true) {
		feedback::update(&state, &scope, |service| {
			service.embedded().worker(&digest)?;
			if input.cancelled == Some(true)
				&& !matches!(
					service.planning().job(id)?.status,
					JobStatus::CancelRequested | JobStatus::Cancelled
				) {
				return Err(PlanningError::Conflict.into());
			}
			service.planning_mut().finish(
				id,
				&worker.id,
				&lease,
				input.failure.as_deref(),
				now(),
			)?;
			Ok(())
		})
		.await?;

		return Ok(Json(json!({"ok":true})));
	}

	if job.kind == JobKind::Plan {
		let plan = input
			.plan
			.ok_or_else(|| ApiFailure::invalid("a planning job must return a plan"))?;
		feedback::update(&state, &scope, |service| {
			service.embedded().worker(&digest)?;
			service
				.planning_mut()
				.complete_plan(id, &worker.id, &lease, plan.clone(), now())?;
			Ok(())
		})
		.await?;

		return Ok(Json(json!({"ok":true})));
	}

	let changes = input
		.changes
		.ok_or_else(|| ApiFailure::invalid("an implementation job must return file changes"))?;
	let result_digest = fingerprint(
		&serde_json::to_string(&changes)
			.map_err(|_| ApiFailure::invalid("invalid file changes"))?,
	);

	if job.status == JobStatus::Completed && job.result_digest.as_deref() == Some(&result_digest) {
		return Ok(Json(json!({"ok":true,"pull_request":job.pull_request})));
	}

	let plan = job
		.plan
		.as_ref()
		.ok_or_else(|| ApiFailure::invalid("the job has no approved plan"))?;
	let mut paths = std::collections::BTreeSet::new();

	if changes.is_empty()
		|| changes.len() > 100
		|| changes.iter().any(|file| {
			!plan.allows_path(&file.path)
				|| !paths.insert(&file.path)
				|| file
					.content
					.as_ref()
					.is_some_and(|content| content.contains('\0'))
		}) {
		return Err(ApiFailure::invalid(
			"changes must be regular text files within the approved paths",
		));
	}
	feedback::update(&state, &scope, |service| {
		service.embedded().worker(&digest)?;
		let version = admin::brief_version(
			service,
			&job.brief.item,
			&job.brief.repository,
			&job.brief.commit,
		)?;
		if version != job.brief.version {
			return Err(PlanningError::Conflict.into());
		}
		if service
			.item(&job.brief.item)
			.is_none_or(|item| !matches!(item.stage, Stage::Accepted | Stage::Building))
		{
			return Err(PlanningError::Conflict.into());
		}
		// Reconcile a persisted, identical result after reconnecting; this never reruns an agent.
		if let Some(current) = service
			.planning_mut()
			.jobs
			.iter_mut()
			.find(|job| job.id == id)
			&& current.status == JobStatus::Publishing
			&& current.result_digest.as_deref() == Some(&result_digest)
			&& current.worker.as_deref() == Some(&worker.id)
			&& current.lease.as_deref() == Some(&lease)
		{
			current.lease_until = now() + 120;
		}
		service
			.planning_mut()
			.heartbeat(id, &worker.id, &lease, now())?;
		let current = service
			.planning_mut()
			.jobs
			.iter_mut()
			.find(|job| job.id == id)
			.ok_or(PlanningError::NotFound)?;
		if current.status == JobStatus::CancelRequested
			|| current
				.result_digest
				.as_ref()
				.is_some_and(|prior| prior != &result_digest)
		{
			return Err(PlanningError::Conflict.into());
		}
		current.status = JobStatus::Publishing;
		current.result_digest = Some(result_digest.clone());
		Ok(())
	})
	.await?;
	let issue = loaded
		.service
		.item(&job.brief.item)
		.and_then(|item| item.issue.as_ref())
		.ok_or_else(|| ApiFailure::invalid("accepted feedback must have an issue"))?;
	let target = scope
		.repositories
		.iter()
		.find(|repository| repository.full_name == job.brief.repository)
		.ok_or_else(|| ApiFailure::invalid("repository is disconnected"))?;
	let app = state
		.github_app
		.as_ref()
		.ok_or_else(|| ApiFailure::invalid("GitHub is not configured"))?;
	let token = app
		.installation_token(&state.http, target.github_installation_id)
		.await
		.map_err(feedback::FeedbackError::from)?;
	let title = format!(
		"feat(feedback): {}",
		plan.summary
			.lines()
			.next()
			.unwrap_or("implement accepted feedback")
			.chars()
			.take(100)
			.collect::<String>()
	);
	let branch = format!("monochange/feedback/{}/{id}", path.project);
	let body = format!(
		"Closes {}#{}\n\nImplemented from a maintainer-approved plan.\n\nFeedback-Item: {}",
		issue.repository.as_deref().unwrap_or(&job.brief.repository),
		issue.number,
		job.brief.item
	);
	let marker = format!("Feedback-Result: {result_digest}");
	let (number, url) = create_feedback_pull_request(
		&state.http,
		&app.api_url,
		&token,
		&FeedbackPullRequest {
			repository: &job.brief.repository,
			base: &job.brief.commit,
			branch: &branch,
			title: &title,
			body: &body,
			marker: &marker,
			files: &changes,
		},
	)
	.await
	.map_err(feedback::FeedbackError::from)?;
	feedback::update(&state, &scope, |service| {
		service.embedded().worker(&digest)?;
		service
			.planning_mut()
			.finish(id, &worker.id, &lease, None, now())?;
		let current = service
			.planning_mut()
			.jobs
			.iter_mut()
			.find(|job| job.id == id)
			.ok_or(PlanningError::NotFound)?;
		current.pull_request = Some(url.clone());
		record_pull_request(
			service,
			&job.brief.item,
			PullRequestRef {
				repository: Some(job.brief.repository.clone()),
				number,
				url: url.clone(),
			},
		)
	})
	.await?;
	Ok(Json(json!({"ok":true,"pull_request":url})))
}

pub(super) fn record_pull_request(
	service: &mut monochange_app_feedback::FeedbackService<
		monochange_app_feedback::RuleBasedTriage,
	>,
	item: &str,
	pull_request: PullRequestRef,
) -> Result<(), monochange_app_feedback::ServiceError> {
	let stage = service
		.item(item)
		.map(|item| item.stage)
		.ok_or_else(|| monochange_app_feedback::ServiceError::NotFound(item.into()))?;

	match stage {
		Stage::Accepted => {
			service
				.start_build(item)
				.and_then(|_| service.open_pull_request(item, pull_request))
				.map(|_| ())
		}
		Stage::Building => service.open_pull_request(item, pull_request).map(|_| ()),
		_ => Ok(()),
	}
}
