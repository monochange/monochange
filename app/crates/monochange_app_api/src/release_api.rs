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
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use monochange_core::HostedCommitRequest;
use monochange_core::HostedCommitResponse;
use monochange_core::HostedReleaseRequest;
use monochange_core::HostedReleaseResponse;
use serde::Serialize;
use sqlx::Row;

use crate::AppState;
use crate::RELEASE_COMMIT_BODY_LIMIT;
use crate::RELEASE_REQUEST_BODY_LIMIT;
use crate::github_app;

/// A repository row joined with its installation.
#[derive(Debug)]
struct ConnectedRepository {
	installation_github_id: i64,
}

/// Resolve the repository by full name and return its installation id.
async fn find_connected_repository(
	state: &AppState,
	full_name: &str,
) -> Result<Option<ConnectedRepository>, String> {
	let row = sqlx::query(
		"SELECT i.github_installation_id
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
			installation_github_id: row.get("github_installation_id"),
		}
	}))
}

/// The caller identity accepted by the hosted endpoints.
#[derive(Debug, PartialEq, Eq)]
enum Caller {
	/// A GitHub Actions run, proven by an OIDC token for this repository.
	ActionsRun { repository: String },
	/// A `MONOCHANGE_TOKEN` API token.
	ApiToken,
}

/// Validate the bearer credentials of a hosted endpoint call.
///
/// This runs before the request body is read, so a caller without valid
/// credentials can never make the server buffer a release payload.
async fn authenticate(
	state: &AppState,
	headers: &axum::http::HeaderMap,
) -> Result<Caller, (StatusCode, Json<ApiError>)> {
	let reject = |status: StatusCode, message: String| (status, Json(ApiError::new(message)));
	let authorization = headers
		.get(AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.ok_or_else(|| {
			reject(
				StatusCode::UNAUTHORIZED,
				"set an Authorization: Bearer header to a GitHub Actions OIDC token or MONOCHANGE_TOKEN"
					.to_string(),
			)
		})?;

	// API tokens are long random strings; OIDC tokens are JWTs with dots.
	if authorization.split('.').count() == 3 {
		let claims = state.oidc.verify(authorization).await.map_err(|error| {
			reject(
				StatusCode::UNAUTHORIZED,
				format!("OIDC verification failed: {error}"),
			)
		})?;

		Ok(Caller::ActionsRun {
			repository: claims.repository,
		})
	} else {
		validate_api_token(state.api_token.as_deref(), authorization)
			.map_err(|(status, message)| reject(status, message))?;
		Ok(Caller::ApiToken)
	}
}

/// Check that an authenticated caller may act on `repository`.
///
/// An OIDC token only grants access to the repository whose workflow minted
/// it; the API token only grants access to the repositories the deployment
/// listed in `MONOCHANGE_TOKEN_REPOSITORIES`.
fn authorize(
	state: &AppState,
	caller: &Caller,
	repository: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
	match caller {
		Caller::ActionsRun {
			repository: claimed,
		} if claimed != repository => {
			Err((
				StatusCode::FORBIDDEN,
				Json(ApiError::new(format!(
					"OIDC token is for repository `{claimed}` but the request targets `{repository}`"
				))),
			))
		}
		Caller::ApiToken
			if !state
				.api_token_repositories
				.as_ref()
				.is_some_and(|repositories| repositories.contains(&repository.to_lowercase())) =>
		{
			Err((
				StatusCode::FORBIDDEN,
				Json(ApiError::new(format!(
					"MONOCHANGE_TOKEN is not scoped to `{repository}`; add the repository to MONOCHANGE_TOKEN_REPOSITORIES or use a GitHub Actions OIDC token"
				))),
			))
		}
		_ => Ok(()),
	}
}

/// Read and parse a JSON request body of at most `limit` bytes.
///
/// Called only after [`authenticate`] succeeds, so unauthenticated callers
/// never make the server buffer their body.
async fn read_json_body<T: serde::de::DeserializeOwned>(
	body: axum::body::Body,
	limit: usize,
) -> Result<T, (StatusCode, Json<ApiError>)> {
	let bytes = axum::body::to_bytes(body, limit).await.map_err(|_| {
		(
			StatusCode::PAYLOAD_TOO_LARGE,
			Json(ApiError::new(format!(
				"request body is larger than the {limit}-byte limit or could not be read"
			))),
		)
	})?;

	serde_json::from_slice(&bytes).map_err(|error| {
		(
			StatusCode::BAD_REQUEST,
			Json(ApiError::new(format!("invalid request body: {error}"))),
		)
	})
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

	if crate::github_app::constant_time_eq(expected.as_bytes(), token.as_bytes()) {
		Ok(())
	} else {
		Err((
			StatusCode::UNAUTHORIZED,
			"invalid MONOCHANGE_TOKEN".to_string(),
		))
	}
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

		if path.contains('\\')
			|| path
				.split('/')
				.any(|component| matches!(component, "" | "." | ".."))
		{
			return Err(format!(
				"release file path `{path}` must be repository-relative"
			));
		}

		if path == ".git" || path.starts_with(".git/") {
			return Err(format!("release file path `{path}` must not touch .git"));
		}

		if matches!(path.as_str(), ".github" | ".github/workflows")
			|| path.starts_with(".github/workflows/")
		{
			return Err(format!(
				"release file path `{path}` must not modify GitHub workflows"
			));
		}
	}

	Ok(())
}

/// Require exactly one source for the release commit's tree: uploaded file
/// contents, or a tree the workflow already created with its own token.
///
/// A tree request carries no paths to check here; the commit step compares
/// the tree's `.github/workflows` with the base commit's before committing.
fn validate_commit_contents(request: &HostedCommitRequest) -> Result<(), String> {
	match (&request.tree, request.files.is_empty()) {
		(Some(_), false) => Err("send either release `files` or a `tree`, not both".to_string()),
		(None, true) => Err("the release commit has no files and no `tree`".to_string()),
		(Some(tree), true)
			if !matches!(tree.len(), 40 | 64)
				|| !tree.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
		{
			Err(format!("`tree` must be a git object id, received `{tree}`"))
		}
		_ => Ok(()),
	}
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
/// rerunning from the current base, and a tree that changes workflows is a
/// rejected request; everything else is a GitHub failure.
fn release_commit_error_status(error: &github_app::GitHubAppError) -> StatusCode {
	match error {
		github_app::GitHubAppError::BranchMoved(..) | github_app::GitHubAppError::BaseMoved(..) => {
			StatusCode::CONFLICT
		}
		github_app::GitHubAppError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
		github_app::GitHubAppError::WorkflowsChanged
		| github_app::GitHubAppError::InvalidReleaseBranch(_)
		| github_app::GitHubAppError::UnboundPullRequest => StatusCode::UNPROCESSABLE_ENTITY,
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
	body: axum::body::Body,
) -> Result<Json<HostedCommitResponse>, (StatusCode, Json<ApiError>)> {
	let caller = authenticate(&state, &headers).await?;
	let request: HostedCommitRequest = read_json_body(body, RELEASE_COMMIT_BODY_LIMIT).await?;

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
		.and_then(|()| validate_commit_contents(&request))
		.and_then(|()| validate_commit_paths(&request))
		.map_err(|message| (StatusCode::BAD_REQUEST, Json(ApiError::new(message))))?;

	let full_name = format!("{}/{}", request.owner, request.repository);
	authorize(&state, &caller, &full_name)?;
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
	body: axum::body::Body,
) -> Result<Json<HostedReleaseResponse>, (StatusCode, Json<ApiError>)> {
	let caller = authenticate(&state, &headers).await?;
	let payload: HostedReleaseRequest = read_json_body(body, RELEASE_REQUEST_BODY_LIMIT).await?;
	let request = &payload.request;
	validate_branch_names(&request.head_branch, Some(&request.base_branch))
		.map_err(|message| (StatusCode::BAD_REQUEST, Json(ApiError::new(message))))?;
	let full_name = format!("{}/{}", request.owner, request.repo);
	authorize(&state, &caller, &full_name)?;

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

	let (number, operation, url) = github_app::publish_release_pull_request(
		&state.http,
		&app.api_url,
		&token,
		request,
		&state.jwt_secret,
		repository.github_repo_id,
	)
	.await
	.map_err(|error| {
		(
			release_commit_error_status(&error),
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
