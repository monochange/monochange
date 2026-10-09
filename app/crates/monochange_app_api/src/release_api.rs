//! Hosted release commit and release request endpoints.
//!
//! These are the endpoints the monochange CLI calls when a workflow configures
//! `commit_backend = "hosted"` or `backend = "hosted"`. Authentication is a
//! GitHub Actions OIDC token (preferred) or a `MONOCHANGE_TOKEN` API token;
//! the caller's repository must have the monochange GitHub App installed.

#[cfg(test)]
#[path = "__tests__/release_api_tests.rs"]
mod tests;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use monochange_core::HostedCommitRequest;
use monochange_core::HostedCommitResponse;
use monochange_core::HostedReleaseRequest;
use monochange_core::HostedReleaseResponse;
use serde::Serialize;
use sqlx::Row;

use crate::AppState;
use crate::github_app;
use crate::oidc;

/// A repository row joined with its installation.
#[derive(Debug)]
struct ConnectedRepository {
	github_repo_id: i64,
	full_name: String,
	installation_github_id: i64,
}

/// Resolve the repository by full name and return its installation id.
async fn find_connected_repository(
	state: &AppState,
	full_name: &str,
) -> Result<Option<ConnectedRepository>, String> {
	let row = sqlx::query(
		"SELECT r.github_repo_id, r.github_full_name, i.github_installation_id
		 FROM repositories r
		 JOIN installations i ON r.installation_id = i.id
		 WHERE r.github_full_name = $1",
	)
	.bind(full_name)
	.fetch_optional(&state.db)
	.await
	.map_err(|error| format!("repository lookup failed: {error}"))?;
	Ok(row.map(|row| {
		ConnectedRepository {
			github_repo_id: row.get("github_repo_id"),
			full_name: row.get("github_full_name"),
			installation_github_id: row.get("github_installation_id"),
		}
	}))
}

/// The caller identity accepted by the hosted endpoints.
enum Caller {
	/// A GitHub Actions run for the repository, proven by an OIDC token.
	ActionsRun,
	/// A `MONOCHANGE_TOKEN` API token.
	ApiToken,
}

/// Extract and validate the bearer credentials of a hosted endpoint call.
async fn resolve_caller(
	state: &AppState,
	headers: &axum::http::HeaderMap,
	repository: &str,
) -> Result<Caller, (StatusCode, String)> {
	let authorization = headers
		.get(AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.map(ToString::to_string)
		.ok_or_else(|| {
			(
				StatusCode::UNAUTHORIZED,
				"set an Authorization: Bearer header to a GitHub Actions OIDC token or MONOCHANGE_TOKEN"
					.to_string(),
			)
		})?;

	// API tokens are long random strings; OIDC tokens are JWTs with dots.
	if authorization.split('.').count() == 3 {
		let verifier = oidc::OidcVerifier::new(&state.oidc_audience);
		let claims = verifier.verify(&authorization).await.map_err(|error| {
			(
				StatusCode::UNAUTHORIZED,
				format!("OIDC verification failed: {error}"),
			)
		})?;
		if claims.repository != repository {
			return Err((
				StatusCode::FORBIDDEN,
				format!(
					"OIDC token is for repository `{}` but the request targets `{repository}`",
					claims.repository
				),
			));
		}
		Ok(Caller::ActionsRun)
	} else {
		validate_api_token(state.api_token.as_deref(), &authorization)?;
		Ok(Caller::ApiToken)
	}
}

/// Validate a `MONOCHANGE_TOKEN` against the token configured through the
/// application secrets.
///
/// The comparison is constant-time so tokens cannot be probed byte by byte.
fn validate_api_token(expected: Option<&str>, token: &str) -> Result<(), (StatusCode, String)> {
	let expected = expected.ok_or_else(|| {
		(
			StatusCode::SERVICE_UNAVAILABLE,
			"MONOCHANGE_TOKEN authentication is not configured on this deployment".to_string(),
		)
	})?;
	if constant_time_eq(expected.as_bytes(), token.as_bytes()) {
		Ok(())
	} else {
		Err((
			StatusCode::UNAUTHORIZED,
			"invalid MONOCHANGE_TOKEN".to_string(),
		))
	}
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
	if left.len() != right.len() {
		return false;
	}
	left.iter()
		.zip(right)
		.fold(0u8, |acc, (a, b)| acc | (a ^ b))
		== 0
}

fn app_unconfigured() -> (StatusCode, String) {
	(
		StatusCode::SERVICE_UNAVAILABLE,
		"the monochange GitHub App is not configured on this deployment".to_string(),
	)
}

/// Error response body used by both endpoints.
#[derive(Debug, Serialize)]
pub struct ApiError {
	pub message: String,
}

impl ApiError {
	fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
		}
	}
}

/// Reject release-managed path candidates that would escape the commit.
///
/// Only repository-relative, non-workflow paths are accepted; mutating
/// `.github/workflows` from a hosted commit would let a release PR change the
/// repository's CI, which the bot must never do on its own.
fn validate_commit_paths(request: &HostedCommitRequest) -> Result<(), String> {
	for file in &request.files {
		let path = &file.path;
		if path.starts_with('/') || path.contains("..") || path.contains('\\') {
			return Err(format!(
				"release file path `{path}` must be repository-relative"
			));
		}
		if path == ".git" || path.starts_with(".git/") {
			return Err(format!("release file path `{path}` must not touch .git"));
		}
		if path.starts_with(".github/workflows/") {
			return Err(format!(
				"release file path `{path}` must not modify GitHub workflows"
			));
		}
	}
	Ok(())
}

/// Reject branch names that are not plausible Git refs.
///
/// Branch names are interpolated into GitHub API paths and queries, so
/// anything that could change the URL (`?`, `#`, `%`, `..`) or that Git
/// itself forbids is refused before any request is made.
fn validate_branch_name(field: &str, branch: &str) -> Result<(), String> {
	let forbidden = |character: char| {
		character.is_control()
			|| matches!(
				character,
				' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '#' | '%'
			)
	};
	if branch.is_empty()
		|| branch.starts_with(['/', '-'])
		|| branch.ends_with(['/', '.'])
		|| branch.strip_suffix(".lock").is_some()
		|| branch.contains("..")
		|| branch.contains("//")
		|| branch.contains("@{")
		|| branch.contains(forbidden)
	{
		return Err(format!("{field} `{branch}` is not a valid branch name"));
	}
	Ok(())
}

/// Validate the release and base branch names of a release request.
fn validate_branch_names(branch: &str, base_branch: Option<&str>) -> Result<(), String> {
	validate_branch_name("branch", branch)?;
	base_branch.map_or(Ok(()), |base_branch| {
		validate_branch_name("base branch", base_branch)
	})
}

/// Map a release commit failure to its HTTP status.
///
/// A moved release or base branch is a conflict the caller resolves by
/// rerunning from the current base; everything else is a GitHub failure.
fn release_commit_error_status(error: &github_app::GitHubAppError) -> StatusCode {
	match error {
		github_app::GitHubAppError::BranchMoved(..) | github_app::GitHubAppError::BaseMoved(..) => {
			StatusCode::CONFLICT
		}
		github_app::GitHubAppError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
		_ => StatusCode::BAD_GATEWAY,
	}
}

/// `POST /api/release-commits` — create a release commit through the bot.
///
/// The caller proves the run belongs to the repository (OIDC) or presents an
/// API token; the server checks that the release branch may move (see
/// [`github_app::create_release_commit`]), then creates blobs, tree, and
/// commit through the Git Database API with an installation token so GitHub
/// signs the result.
pub async fn create_release_commit(
	State(state): State<AppState>,
	headers: axum::http::HeaderMap,
	payload: Result<Json<HostedCommitRequest>, JsonRejection>,
) -> Result<Json<HostedCommitResponse>, (StatusCode, Json<ApiError>)> {
	let Json(request) = payload.map_err(|error| {
		(
			StatusCode::BAD_REQUEST,
			Json(ApiError::new(format!("invalid request body: {error}"))),
		)
	})?;
	if request.provider != "github" {
		return Err((
			StatusCode::BAD_REQUEST,
			Json(ApiError::new(format!(
				"provider `{}` is not supported for hosted commits; only `github` is hosted today",
				request.provider
			))),
		));
	}
	validate_branch_names(&request.branch, request.base_branch.as_deref())
		.and_then(|()| validate_commit_paths(&request))
		.map_err(|message| (StatusCode::BAD_REQUEST, Json(ApiError::new(message))))?;

	let full_name = format!("{}/{}", request.owner, request.repository);
	let repository = find_connected_repository(&state, &full_name)
		.await
		.map_err(|message| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(message))))?
		.ok_or_else(|| {
			(
				StatusCode::NOT_FOUND,
				Json(ApiError::new(format!(
					"repository `{full_name}` has no monochange GitHub App installation; install the monochange app on the repository to use hosted release commits"
				))),
			)
		})?;
	if repository.github_repo_id.to_string() != request.repository
		&& repository.full_name != full_name
	{
		return Err((
			StatusCode::BAD_REQUEST,
			Json(ApiError::new("repository mismatch")),
		));
	}
	resolve_caller(&state, &headers, &full_name)
		.await
		.map_err(|(status, message)| (status, Json(ApiError::new(message))))?;

	let app = state
		.github_app
		.as_ref()
		.ok_or_else(app_unconfigured)
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

	if request.dry_run {
		return Ok(Json(HostedCommitResponse {
			commit: None,
			verified: false,
			status: Some("dry_run".to_string()),
			message: None,
		}));
	}

	let response = commit_release_files(&state.http, &app.api_url, &token, &request).await?;
	Ok(Json(response))
}

/// Create the release commit through the GitHub App and shape the response.
///
/// Reports GitHub's verification result and maps a moved release or base
/// branch to `409 Conflict`.
async fn commit_release_files(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	request: &HostedCommitRequest,
) -> Result<HostedCommitResponse, (StatusCode, Json<ApiError>)> {
	let (sha, verified, reason) =
		github_app::create_release_commit(http, api_url, installation_token, request)
			.await
			.map_err(|error| {
				(
					release_commit_error_status(&error),
					Json(ApiError::new(error.to_string())),
				)
			})?;

	let repository = format!("{}/{}", request.owner, request.repository);
	tracing::info!(
		repository = %repository,
		branch = %request.branch,
		commit = %sha,
		verified,
		"hosted release commit created"
	);

	Ok(HostedCommitResponse {
		commit: Some(sha),
		verified,
		status: Some("completed".to_string()),
		message: reason,
	})
}

/// `POST /api/release-requests` — open or update the release pull request.
pub async fn publish_release_request(
	State(state): State<AppState>,
	headers: axum::http::HeaderMap,
	payload: Result<Json<HostedReleaseRequest>, JsonRejection>,
) -> Result<Json<HostedReleaseResponse>, (StatusCode, Json<ApiError>)> {
	let Json(payload) = payload.map_err(|error| {
		(
			StatusCode::BAD_REQUEST,
			Json(ApiError::new(format!("invalid request body: {error}"))),
		)
	})?;
	let request = &payload.request;
	validate_branch_names(&request.head_branch, Some(&request.base_branch))
		.map_err(|message| (StatusCode::BAD_REQUEST, Json(ApiError::new(message))))?;
	let full_name = format!("{}/{}", request.owner, request.repo);

	let repository = find_connected_repository(&state, &full_name)
		.await
		.map_err(|message| {
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(ApiError::new(message)),
			)
		})?
		.ok_or_else(|| {
			(
				StatusCode::NOT_FOUND,
				Json(ApiError::new(format!(
					"repository `{full_name}` has no monochange GitHub App installation"
				))),
			)
		})?;
	resolve_caller(&state, &headers, &full_name)
		.await
		.map_err(|(status, message)| (status, Json(ApiError::new(message))))?;

	let app = state
		.github_app
		.as_ref()
		.ok_or_else(app_unconfigured)
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

	if payload.dry_run {
		return Ok(Json(HostedReleaseResponse {
			number: None,
			operation: None,
			url: None,
			head_branch: Some(request.head_branch.clone()),
			message: Some("dry_run".to_string()),
		}));
	}

	let (number, operation, url) =
		github_app::publish_release_pull_request(&state.http, &app.api_url, &token, request)
			.await
			.map_err(|error| {
				(
					StatusCode::BAD_GATEWAY,
					Json(ApiError::new(error.to_string())),
				)
			})?;

	tracing::info!(
		repository = %full_name,
		number,
		operation = ?operation,
		"hosted release pull request published"
	);

	Ok(Json(HostedReleaseResponse {
		number: Some(number),
		operation: Some(operation),
		url: Some(url),
		head_branch: Some(request.head_branch.clone()),
		message: None,
	}))
}
