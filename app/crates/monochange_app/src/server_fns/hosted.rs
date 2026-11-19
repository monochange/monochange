//! Hosted release operations for CI: the monochange CLI's RPC.
//!
//! The release commit, release pull request, and workflow dispatch a
//! workflow asks monochange.dev to perform through the monochange GitHub
//! App. They are leptoskit RPC operations, published at
//! `/api/<endpoint>` with JSON arguments and results, so generated clients
//! and the CLI share one described contract.
//!
//! Their callers are workflows, not browser sessions. Each request carries a
//! GitHub Actions OIDC token or a `MONOCHANGE_TOKEN` as its bearer, and
//! [`machine_gate`] verifies it before the body is read, raises the body
//! limit for release payloads, and hands the verified [`Caller`] to the
//! operation. A failure answers with its HTTP status and a
//! [`HostedRpcError`] JSON body, the same statuses the REST routes these
//! operations replace return.
//!
//! The REST routes (`/api/release-commits`, `/api/release-requests`,
//! `/api/workflow-dispatches`) stay for older CLIs and run the same cores.

#[cfg(test)]
#[path = "__tests__/hosted_tests.rs"]
mod tests;

use std::sync::Arc;

use axum::Extension;
use axum::extract::DefaultBodyLimit;
use axum::extract::Request;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use leptos::prelude::expect_context;
use leptos::server;
use leptoskit_rpc::api;
use monochange_app_api::AppState;
use monochange_app_api::RELEASE_COMMIT_BODY_LIMIT;
use monochange_app_api::RELEASE_REQUEST_BODY_LIMIT;
use monochange_app_api::release_api;
use monochange_app_api::release_api::ApiError;
use monochange_app_api::release_api::Caller;
use monochange_app_api::workflow_api;
use monochange_app_api::workflow_api::WORKFLOW_DISPATCH_BODY_LIMIT;
use monochange_core::HostedCommitRequest;
use monochange_core::HostedCommitResponse;
// The operation's own argument struct is `HostedReleaseRequest`.
use monochange_core::HostedReleaseRequest as ReleaseRequestPayload;
use monochange_core::HostedReleaseResponse;
use monochange_core::HostedWorkflowDispatchRequest;
use monochange_core::HostedWorkflowDispatchResponse;
use serde::Deserialize;
use serde::Serialize;
use server_fn::ServerFn;
use server_fn::codec::Json;
use server_fn::codec::JsonEncoding;
use server_fn::error::FromServerFnError;
use server_fn::error::ServerFnErrorErr;

/// A hosted operation's failure: the HTTP status it answered with and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedRpcError {
	/// The HTTP status, such as `401` or `409`.
	pub status: u16,
	/// What went wrong, for the person reading the workflow log.
	pub message: String,
}

impl HostedRpcError {
	fn new(status: StatusCode, message: impl Into<String>) -> Self {
		Self {
			status: status.as_u16(),
			message: message.into(),
		}
	}
}

impl std::fmt::Display for HostedRpcError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "HTTP {}: {}", self.status, self.message)
	}
}

impl From<(StatusCode, axum::Json<ApiError>)> for HostedRpcError {
	fn from((status, axum::Json(error)): (StatusCode, axum::Json<ApiError>)) -> Self {
		Self::new(status, error.message)
	}
}

impl FromServerFnError for HostedRpcError {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		let status = match &value {
			ServerFnErrorErr::Deserialization(message) if message.contains("size limit") => {
				StatusCode::PAYLOAD_TOO_LARGE
			}
			ServerFnErrorErr::Args(_)
			| ServerFnErrorErr::MissingArg(_)
			| ServerFnErrorErr::Deserialization(_) => StatusCode::BAD_REQUEST,
			_ => StatusCode::INTERNAL_SERVER_ERROR,
		};

		Self::new(status, value.to_string())
	}

	fn status_code(&self) -> StatusCode {
		StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
	}
}

/// Create a release commit on the release branch through the monochange
/// GitHub App, so GitHub signs it as the bot.
///
/// The workflow sends the release files, or a tree it created itself, and
/// the server checks the paths, the caller's access to the repository, and
/// that the release and base branches have not moved.
///
/// # Arguments
///
/// * `request` - The release commit the workflow prepared.
///
/// # Errors
///
/// `400` for an invalid request, `401` for a missing or rejected token, `403`
/// when the token may not act on the repository, `404` when the repository
/// has no monochange GitHub App installation, `409` when a branch moved,
/// `413` when the body is too large, `422` when the tree changes workflows,
/// `503` when the deployment has no GitHub App, and `502` for GitHub failures.
#[api(crate = "leptoskit_rpc", tag = "hosted")]
#[server(endpoint = "hosted_release_commit", input = Json, output = Json)]
pub async fn hosted_release_commit(
	request: HostedCommitRequest,
) -> Result<HostedCommitResponse, HostedRpcError> {
	let (state, caller) = machine_context().await?;
	release_api::release_commit(&state, &caller, request)
		.await
		.map_err(HostedRpcError::from)
}

/// Open or update the release pull request through the monochange GitHub
/// App, so the pull request's events run the repository's workflows.
///
/// # Arguments
///
/// * `request` - The release pull request the workflow rendered.
///
/// # Errors
///
/// The statuses of `hosted_release_commit`.
#[api(crate = "leptoskit_rpc", tag = "hosted")]
#[server(endpoint = "hosted_release_request", input = Json, output = Json)]
pub async fn hosted_release_request(
	request: ReleaseRequestPayload,
) -> Result<HostedReleaseResponse, HostedRpcError> {
	let (state, caller) = machine_context().await?;
	release_api::release_request(&state, &caller, request)
		.await
		.map_err(HostedRpcError::from)
}

/// Dispatch a GitHub Actions workflow in a repository of the same monochange
/// GitHub App installation as the calling workflow.
///
/// # Arguments
///
/// * `request` - The workflow, ref, and inputs to dispatch.
///
/// # Errors
///
/// `400` for an invalid request, `401` for a missing or rejected token, `403`
/// when the caller may not dispatch in the target repository, `404` when the
/// repository or workflow is missing, `422` when GitHub rejects the inputs,
/// `503` when the deployment has no GitHub App, and `502` for other GitHub
/// failures.
#[api(crate = "leptoskit_rpc", tag = "hosted")]
#[server(endpoint = "hosted_workflow_dispatch", input = Json, output = Json)]
pub async fn hosted_workflow_dispatch(
	request: HostedWorkflowDispatchRequest,
) -> Result<HostedWorkflowDispatchResponse, HostedRpcError> {
	let (state, caller) = machine_context().await?;
	workflow_api::workflow_dispatch(&state, &caller, request)
		.await
		.map_err(HostedRpcError::from)
}

/// The app state and the caller [`machine_gate`] verified.
async fn machine_context() -> Result<(Arc<AppState>, Caller), HostedRpcError> {
	let state = expect_context::<Arc<AppState>>();
	let Extension(caller) = leptos_axum::extract::<Extension<Caller>>()
		.await
		.map_err(|_| {
			HostedRpcError::new(
				StatusCode::UNAUTHORIZED,
				"the request was not authenticated as a workflow",
			)
		})?;

	Ok((state, caller))
}

/// The hosted operations' paths and the largest body each accepts.
fn machine_paths() -> [(&'static str, usize); 3] {
	[
		(HostedReleaseCommit::PATH, RELEASE_COMMIT_BODY_LIMIT),
		(HostedReleaseRequest::PATH, RELEASE_REQUEST_BODY_LIMIT),
		(HostedWorkflowDispatch::PATH, WORKFLOW_DISPATCH_BODY_LIMIT),
	]
}

/// Whether `path` is a hosted operation, authenticated by [`machine_gate`]
/// rather than by a browser session or client token.
#[must_use]
pub fn is_machine_path(path: &str) -> bool {
	machine_paths()
		.iter()
		.any(|(machine_path, _)| *machine_path == path)
}

/// Authenticate a hosted operation's workflow before its body is read.
///
/// Requests to other paths pass through untouched. A hosted request without
/// a valid OIDC token or `MONOCHANGE_TOKEN` is answered here, so it never
/// makes the server buffer a release payload. An accepted request carries
/// its [`Caller`] to the operation and may send a body up to the operation's
/// own limit.
pub async fn machine_gate(
	State(state): State<Arc<AppState>>,
	mut request: Request,
	next: Next,
) -> Response {
	let path = request.uri().path().to_owned();
	let Some((_, limit)) = machine_paths()
		.into_iter()
		.find(|(machine_path, _)| *machine_path == path)
	else {
		return next.run(request).await;
	};

	let caller = match release_api::authenticate(&state, request.headers()).await {
		Ok(caller) => caller,
		Err(rejection) => return rejection_response(&HostedRpcError::from(rejection)),
	};

	request.extensions_mut().insert(caller);
	// `DefaultBodyLimit` only stores the limit in the request's extensions,
	// where the server function's body reader finds it.
	let service = tower::ServiceBuilder::new()
		.layer(DefaultBodyLimit::max(limit))
		.service(next);
	tower::ServiceExt::oneshot(service, request)
		.await
		.unwrap_or_else(|never| match never {})
}

fn rejection_response(error: &HostedRpcError) -> Response {
	let parts = error.ser();
	let mut response = (
		[(axum::http::header::CONTENT_TYPE, parts.content_type)],
		axum::body::Body::from(parts.body),
	)
		.into_response();
	*response.status_mut() = parts.status_code;
	response
}
