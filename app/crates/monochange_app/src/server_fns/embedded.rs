//! Maintainer controls for embedded clients, connected workers, and plan review.

use leptos::server;
use monochange_app_feedback::planning::PlanningJob;
use monochange_app_feedback::planning::PlanningPolicy;
use serde::Deserialize;
use serde::Serialize;

#[cfg(test)]
#[path = "__tests__/embedded_tests.rs"]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationView {
	pub id: String,
	pub name: String,
	pub namespace: String,
	pub origins: Vec<String>,
	pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerView {
	pub id: String,
	pub name: String,
	pub repositories: Vec<String>,
	pub last_seen: String,
	pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedView {
	pub project_id: i32,
	pub integrations: Vec<IntegrationView>,
	pub workers: Vec<WorkerView>,
	pub policy: PlanningPolicy,
	pub jobs: Vec<PlanningJob>,
	pub repositories: Vec<String>,
	pub items: Vec<(String, String)>,
	pub assistant_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedReceipt {
	pub message: String,
	pub credential: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
fn required(value: Option<&str>) -> Result<&str, server_fn::ServerFnError> {
	value
		.filter(|value| !value.trim().is_empty())
		.ok_or_else(|| server_fn::ServerFnError::new("Complete the required fields."))
}

#[server]
pub async fn embedded_console(
	organization: String,
	project: String,
) -> Result<EmbeddedView, server_fn::ServerFnError> {
	use std::sync::Arc;
	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let access = super::organizations::maintained_project(&state, &organization, &project)
		.await?
		.ok_or_else(|| server_fn::ServerFnError::new("That project is not available to you."))?;
	let loaded = monochange_app_api::feedback::load(&state, &access.scope)
		.await
		.map_err(super::feedback::feedback_error)?;
	let embedded = loaded.service.embedded();
	let mut jobs = loaded.service.planning().jobs.clone();
	jobs.reverse();

	for job in &mut jobs {
		job.lease = None;
	}

	Ok(EmbeddedView {
		project_id: access.scope.project_id,
		integrations: embedded
			.integrations
			.iter()
			.map(|app| {
				IntegrationView {
					id: app.id.clone(),
					name: app.name.clone(),
					namespace: app.namespace.clone(),
					origins: app.origins.clone(),
					revoked: app.revoked,
				}
			})
			.collect(),
		workers: embedded
			.workers
			.iter()
			.map(|worker| {
				WorkerView {
					id: worker.id.clone(),
					name: worker.name.clone(),
					repositories: worker.repositories.clone(),
					revoked: worker.revoked,
					last_seen: chrono::DateTime::from_timestamp(worker.last_seen, 0)
						.map(|time| time.format("%d %b %Y %H:%M UTC").to_string())
						.unwrap_or_default(),
				}
			})
			.collect(),
		policy: loaded.service.planning().policy.clone(),
		jobs,
		repositories: access
			.scope
			.repositories
			.iter()
			.map(|repository| repository.full_name.clone())
			.collect(),
		items: loaded
			.service
			.items()
			.iter()
			.filter(|item| {
				matches!(
					item.stage,
					monochange_app_feedback::Stage::Voting
						| monochange_app_feedback::Stage::Accepted
				)
			})
			.map(|item| {
				(
					item.id.clone(),
					item.triage
						.as_ref()
						.map_or_else(|| item.id.clone(), |report| report.product_summary.clone()),
				)
			})
			.collect(),
		assistant_enabled: state.feedback_assistant.is_some(),
	})
}

// Leptos maps individual named form fields to server arguments.
#[allow(clippy::too_many_arguments)]
#[server]
pub async fn embedded_action(
	organization: String,
	project: String,
	action: String,
	#[server(default)] id: Option<String>,
	#[server(default)] name: Option<String>,
	#[server(default)] namespace: Option<String>,
	#[server(default)] origins: Option<String>,
	#[server(default)] surfaces: Option<String>,
	#[server(default)] repository: Option<String>,
	#[server(default)] rationale: Option<String>,
	#[server(default)] automatic: bool,
	#[server(default)] daily_limit: Option<usize>,
) -> Result<EmbeddedReceipt, server_fn::ServerFnError> {
	use std::sync::Arc;

	use monochange_app_api::embedded_api::admin;
	use monochange_app_feedback::embedded::ClientSurface;
	use monochange_app_feedback::planning::ReviewDecision;
	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let access = super::organizations::maintained_project(&state, &organization, &project)
		.await?
		.ok_or_else(|| server_fn::ServerFnError::new("That project is not available to you."))?;
	let scope = &access.scope;
	let error = |error: monochange_app_api::embedded_api::ApiFailure| {
		server_fn::ServerFnError::new(error.message)
	};
	let mut credential = None;

	match action.as_str() {
		"register" => {
			let surfaces = match surfaces.as_deref().unwrap_or("both") {
				"web" => vec![ClientSurface::Web],
				"mobile" => vec![ClientSurface::Mobile],
				"both" => vec![ClientSurface::Web, ClientSurface::Mobile],
				_ => {
					return Err(server_fn::ServerFnError::new(
						"Choose web, mobile, or both.",
					));
				}
			};

			let issued = admin::register(
				&state,
				scope,
				admin::IntegrationInput {
					name: required(name.as_deref())?.into(),
					namespace: required(namespace.as_deref())?.into(),
					origins: origins
						.unwrap_or_default()
						.lines()
						.map(str::trim)
						.filter(|origin| !origin.is_empty())
						.map(str::to_owned)
						.collect(),
					surfaces,
				},
			)
			.await
			.map_err(error)?;
			credential = Some(issued.token);
		}
		"rotate" | "revoke_integration" => {
			credential = Some(
				admin::rotate(
					&state,
					scope,
					required(id.as_deref())?,
					action == "revoke_integration",
				)
				.await
				.map_err(error)?
				.token,
			);
		}
		"pair_worker" => {
			credential = Some(
				admin::pairing(
					&state,
					scope,
					required(name.as_deref())?,
					&[required(repository.as_deref())?.into()],
				)
				.await
				.map_err(error)?
				.token,
			);
		}
		"revoke_worker" => {
			admin::revoke_worker(&state, scope, required(id.as_deref())?)
				.await
				.map_err(error)?;
		}
		"policy" => {
			let limit = daily_limit.unwrap_or(10);
			if !(1..=100).contains(&limit) {
				return Err(server_fn::ServerFnError::new(
					"Choose a daily planning limit between 1 and 100.",
				));
			}

			admin::set_policy(
				&state,
				scope,
				automatic,
				repository.as_deref().unwrap_or_default(),
				limit,
				&access.maintainer,
			)
			.await
			.map_err(error)?;
		}
		"plan" => {
			admin::queue_plan(
				&state,
				scope,
				required(id.as_deref())?,
				required(repository.as_deref())?,
			)
			.await
			.map_err(error)?;
		}
		"approve" => {
			admin::approve(&state, scope, required(id.as_deref())?, &access.maintainer)
				.await
				.map_err(error)?;
		}
		"reject" | "changes" => {
			admin::review(
				&state,
				scope,
				required(id.as_deref())?,
				if action == "reject" {
					ReviewDecision::Reject
				} else {
					ReviewDecision::RequestChanges
				},
				&access.maintainer,
				required(rationale.as_deref())?,
			)
			.await
			.map_err(error)?;
		}
		"cancel" => {
			monochange_app_api::feedback::update(&state, scope, |service| {
				service
					.planning_mut()
					.cancel(required(id.as_deref()).map_err(|_| {
						monochange_app_feedback::embedded::EmbeddedError::Invalid("job id")
					})?)?;
				Ok(())
			})
			.await
			.map_err(super::feedback::feedback_error)?;
		}
		"retry" => {
			let job_id = required(id.as_deref())?;
			monochange_app_api::feedback::update(&state, scope, |service| {
				let previous = service.planning().job(job_id)?.clone();
				if admin::brief_version(
					service,
					&previous.brief.item,
					&previous.brief.repository,
					&previous.brief.commit,
				)? != previous.brief.version
				{
					return Err(monochange_app_feedback::planning::PlanningError::Conflict.into());
				}
				service.planning_mut().retry(
					job_id,
					&access.maintainer,
					chrono::Utc::now().timestamp(),
				)?;
				Ok(())
			})
			.await
			.map_err(super::feedback::feedback_error)?;
		}
		"confirm_stopped" => {
			let job_id = required(id.as_deref())?;
			let reason = required(rationale.as_deref())?;
			monochange_app_api::feedback::update(&state, scope, |service| {
				service.planning_mut().confirm_stopped(
					job_id,
					&access.maintainer,
					reason,
					chrono::Utc::now().timestamp(),
				)?;
				Ok(())
			})
			.await
			.map_err(super::feedback::feedback_error)?;
		}

		_ => {
			return Err(server_fn::ServerFnError::new(
				"Unknown feedback integration action.",
			));
		}
	}

	Ok(EmbeddedReceipt {
		message: if credential.as_ref().is_some_and(|value| !value.is_empty()) {
			"Copy this credential now; it is displayed once. Keep it on your backend or worker."
				.into()
		} else {
			"Feedback settings updated.".into()
		},

		credential: credential.filter(|value| !value.is_empty()),
	})
}
