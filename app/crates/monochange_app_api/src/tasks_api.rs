//! The public task API that `monochange tasks` reads.
//!
//! Anyone may read it without signing in: every listed task is public, so
//! anyone can audit what a project asked contributors' agents to do. Only
//! tasks that passed screening, haven't expired, and belong to an opted-in
//! public project appear.

#[cfg(test)]
#[path = "__tests__/tasks_api_tests.rs"]
mod tests;

use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use monochange_app_feedback::PublicTask;
use serde::Deserialize;
use serde::Serialize;

use crate::AppState;
use crate::feedback;
use crate::release_api::ApiError;

/// The open tasks of one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
	pub organization: String,
	pub project: String,
	pub tasks: Vec<PublicTask>,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ApiError>) {
	(
		status,
		Json(ApiError {
			message: message.to_owned(),
		}),
	)
}

/// The project's open tasks at this moment, oldest first.
async fn open_tasks(
	state: &AppState,
	organization: &str,
	project: &str,
) -> Result<(String, String, Vec<PublicTask>), (StatusCode, Json<ApiError>)> {
	let unavailable = |error: feedback::FeedbackError| {
		tracing::warn!(%error, "task listing failed");
		self::error(
			StatusCode::SERVICE_UNAVAILABLE,
			"tasks can't be read right now",
		)
	};
	let public = feedback::public_project(state, organization, project)
		.await
		.map_err(unavailable)?
		.ok_or_else(|| error(StatusCode::NOT_FOUND, "no such project"))?;
	let loaded = feedback::load(state, &public.scope)
		.await
		.map_err(unavailable)?;
	Ok((
		public.organization,
		public.project.slug,
		loaded.service.open_tasks(chrono::Utc::now().timestamp()),
	))
}

/// `GET /api/projects/{organization}/{project}/tasks`
pub async fn list_tasks(
	State(state): State<AppState>,
	Path((organization, project)): Path<(String, String)>,
) -> ApiResult<TaskList> {
	let (organization, project, tasks) = open_tasks(&state, &organization, &project).await?;
	Ok(Json(TaskList {
		organization,
		project,
		tasks,
	}))
}

/// `GET /api/projects/{organization}/{project}/tasks/{task}`
pub async fn show_task(
	State(state): State<AppState>,
	Path((organization, project, task)): Path<(String, String, String)>,
) -> ApiResult<PublicTask> {
	let (_, _, tasks) = open_tasks(&state, &organization, &project).await?;
	tasks
		.into_iter()
		.find(|open| open.id == task)
		.map(Json)
		.ok_or_else(|| {
			error(
				StatusCode::NOT_FOUND,
				"no open task with that id; it may have expired or been withdrawn",
			)
		})
}
