//! Hosted workflow dispatch endpoint.
//!
//! `POST /api/workflow-dispatches` lets a GitHub Actions workflow dispatch a
//! workflow in another repository owned by the same monochange GitHub App
//! installation. Authentication is the same as the hosted release endpoints:
//! a GitHub Actions OIDC token (preferred) or a scoped `MONOCHANGE_TOKEN`.

#[cfg(test)]
#[path = "__tests__/workflow_api_tests.rs"]
mod tests;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use monochange_core::HostedWorkflowDispatchRequest;
use monochange_core::HostedWorkflowDispatchResponse;

use crate::AppState;
use crate::github_app;
use crate::release_api;
use crate::release_api::ApiError;
use crate::release_api::Caller;
use crate::release_api::ConnectedRepository;

/// Largest workflow dispatch request body the app accepts.
///
/// GitHub limits a workflow dispatch to 10 inputs. The cap keeps a hostile
/// caller from making the app buffer an arbitrarily large body before the
/// request is validated.
pub const WORKFLOW_DISPATCH_BODY_LIMIT: usize = 64 * 1024;

/// Validate the target repository, workflow, ref, and inputs.
fn validate_workflow_dispatch_request(
	request: &HostedWorkflowDispatchRequest,
) -> Result<(), String> {
	if request.provider != "github" {
		return Err(format!(
			"provider `{}` is not supported for hosted workflow dispatch; only `github` is hosted today",
			request.provider
		));
	}

	validate_repository_component("owner", &request.owner)?;
	validate_repository_component("repository", &request.repository)?;
	validate_workflow(&request.workflow)?;
	release_api::validate_branch_name("ref", &request.git_ref)?;

	if request.inputs.len() > 10 {
		return Err(format!(
			"a workflow dispatch accepts at most 10 inputs, received {}",
			request.inputs.len()
		));
	}

	for (name, value) in &request.inputs {
		validate_input_name(name)?;

		if value.len() > 1024 {
			return Err(format!(
				"workflow input `{name}` is {} bytes; the maximum is 1024",
				value.len()
			));
		}
	}

	Ok(())
}

fn validate_repository_component(field: &str, value: &str) -> Result<(), String> {
	let valid = !value.is_empty()
		&& value.len() <= 100
		&& value != "."
		&& value != ".."
		&& value.chars().all(|character| {
			character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
		});

	if valid {
		Ok(())
	} else {
		Err(format!(
			"{field} `{value}` is not a valid GitHub repository name"
		))
	}
}

fn validate_workflow(workflow: &str) -> Result<(), String> {
	if workflow.is_empty() || workflow.len() > 255 {
		return Err("workflow must be a file name or numeric workflow id".to_string());
	}

	if workflow.bytes().all(|byte| byte.is_ascii_digit()) {
		return Ok(());
	}

	let extension = std::path::Path::new(workflow)
		.extension()
		.and_then(|extension| extension.to_str());
	let is_workflow_file = extension.is_some_and(|extension| {
		extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
	});
	let valid_name = is_workflow_file
		&& workflow.chars().all(|character| {
			character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
		});
	if valid_name {
		Ok(())
	} else {
		Err(format!(
			"workflow `{workflow}` must be a `.yml` or `.yaml` file name or a numeric workflow id"
		))
	}
}

fn validate_input_name(name: &str) -> Result<(), String> {
	let valid = !name.is_empty()
		&& name.len() <= 100
		&& name
			.chars()
			.all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));

	if valid {
		Ok(())
	} else {
		Err(format!("workflow input name `{name}` is not valid"))
	}
}

/// Resolve a connected repository or return the supplied missing-repository
/// error. Database failures are always a server error.
async fn connected_repository(
	state: &AppState,
	full_name: &str,
	missing_status: StatusCode,
	missing_message: String,
) -> Result<ConnectedRepository, (StatusCode, Json<ApiError>)> {
	release_api::find_connected_repository(state, full_name)
		.await
		.map_err(|message| {
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(ApiError::new(message)),
			)
		})?
		.ok_or_else(|| (missing_status, Json(ApiError::new(missing_message))))
}

/// Authorize a workflow dispatch and return the target repository connection.
///
/// OIDC callers may target any repository in the same GitHub App installation
/// as the repository that minted their token. API-token callers may only
/// target repositories listed in `MONOCHANGE_TOKEN_REPOSITORIES`.
async fn authorize_workflow_dispatch(
	state: &AppState,
	caller: &Caller,
	target: &str,
) -> Result<ConnectedRepository, (StatusCode, Json<ApiError>)> {
	let target_repository = connected_repository(
		state,
		target,
		StatusCode::NOT_FOUND,
		format!("repository `{target}` has no monochange GitHub App installation"),
	)
	.await?;

	match caller {
		Caller::ActionsRun { repository } => {
			let caller_repository = connected_repository(
				state,
				repository,
				StatusCode::FORBIDDEN,
				format!(
					"OIDC token repository `{repository}` has no monochange GitHub App installation"
				),
			)
			.await?;

			if caller_repository.installation_github_id != target_repository.installation_github_id
			{
				return Err((
					StatusCode::FORBIDDEN,
					Json(ApiError::new(format!(
						"OIDC token repository `{repository}` and target `{target}` are not in the same monochange GitHub App installation"
					))),
				));
			}
		}
		Caller::ApiToken
			if !state
				.api_token_repositories
				.as_ref()
				.is_some_and(|repositories| repositories.contains(&target.to_lowercase())) =>
		{
			return Err((
				StatusCode::FORBIDDEN,
				Json(ApiError::new(format!(
					"MONOCHANGE_TOKEN is not scoped to `{target}`; add the repository to MONOCHANGE_TOKEN_REPOSITORIES or use a GitHub Actions OIDC token"
				))),
			));
		}
		Caller::ApiToken => {}
	}

	Ok(target_repository)
}

fn dispatch_error_status(error: &github_app::GitHubAppError) -> StatusCode {
	match error {
		github_app::GitHubAppError::Status(_, status, _)
			if *status == reqwest::StatusCode::NOT_FOUND =>
		{
			StatusCode::NOT_FOUND
		}
		github_app::GitHubAppError::Status(_, status, _)
			if *status == reqwest::StatusCode::UNPROCESSABLE_ENTITY =>
		{
			StatusCode::UNPROCESSABLE_ENTITY
		}
		_ => StatusCode::BAD_GATEWAY,
	}
}

/// `POST /api/workflow-dispatches` — dispatch a GitHub Actions workflow.
pub async fn dispatch_workflow(
	State(state): State<AppState>,
	headers: axum::http::HeaderMap,
	body: axum::body::Body,
) -> Result<Json<HostedWorkflowDispatchResponse>, (StatusCode, Json<ApiError>)> {
	let caller = release_api::authenticate(&state, &headers).await?;
	let request: HostedWorkflowDispatchRequest =
		release_api::read_json_body(body, WORKFLOW_DISPATCH_BODY_LIMIT).await?;

	validate_workflow_dispatch_request(&request)
		.map_err(|message| (StatusCode::BAD_REQUEST, Json(ApiError::new(message))))?;

	let full_name = format!("{}/{}", request.owner, request.repository);
	let repository = authorize_workflow_dispatch(&state, &caller, &full_name).await?;

	let app = state
		.github_app
		.as_ref()
		.ok_or_else(release_api::app_unconfigured)
		.map_err(|(status, message)| (status, Json(ApiError::new(message))))?;
	let token = app
		.installation_token(&state.http, repository.installation_github_id)
		.await
		.map_err(|error| {
			(
				StatusCode::BAD_GATEWAY,
				Json(ApiError::new(format!(
					"failed to mint installation token: {error}"
				))),
			)
		})?;

	let response = HostedWorkflowDispatchResponse {
		status: Some(if request.dry_run {
			"dry_run".to_string()
		} else {
			"dispatched".to_string()
		}),
		workflow: Some(request.workflow.clone()),
		git_ref: Some(request.git_ref.clone()),
		message: None,
	};

	if request.dry_run {
		return Ok(Json(response));
	}

	github_app::dispatch_workflow(
		&state.http,
		&app.api_url,
		&token,
		&full_name,
		&request.workflow,
		&request.git_ref,
		&request.inputs,
	)
	.await
	.map_err(|error| {
		(
			dispatch_error_status(&error),
			Json(ApiError::new(error.to_string())),
		)
	})?;

	tracing::info!(
		repository = %full_name,
		workflow = %request.workflow,
		git_ref = %request.git_ref,
		"hosted workflow dispatched"
	);

	Ok(Json(response))
}
