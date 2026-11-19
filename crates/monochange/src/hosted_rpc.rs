//! Calls to the monochange app's hosted release operations.
//!
//! The app serves them as leptoskit RPC operations: `POST /api/<operation>`
//! with the arguments as a JSON object (`{"request": ...}`) and the result
//! as JSON. A failure answers with its HTTP status and a
//! `{"status": 409, "message": "..."}` body.
//!
//! Deployments from before the RPC operations only serve the REST routes
//! (`/api/release-commits`, ...), which take the request itself as the body.
//! When the RPC route is missing — a `404` without the RPC error body — the
//! call is sent once more to the REST route, so a new CLI keeps working
//! against an older or self-hosted deployment.

#[cfg(test)]
#[path = "__tests__/hosted_rpc_tests.rs"]
mod tests;

use std::time::Duration;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// One hosted operation: its RPC name, the REST route older deployments
/// serve, and how errors name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostedOperation {
	/// The RPC operation, served at `/api/<rpc>`.
	pub(crate) rpc: &'static str,
	/// The REST route older deployments serve, at `/api/<legacy>`.
	pub(crate) legacy: &'static str,
	/// How error messages name the operation, such as `hosted CommitRelease`.
	pub(crate) label: &'static str,
}

/// Create the release commit through the monochange GitHub App.
pub(crate) const RELEASE_COMMIT: HostedOperation = HostedOperation {
	rpc: "hosted_release_commit",
	legacy: "release-commits",
	label: "hosted CommitRelease",
};

/// Open or update the release pull request through the monochange GitHub App.
pub(crate) const RELEASE_REQUEST: HostedOperation = HostedOperation {
	rpc: "hosted_release_request",
	legacy: "release-requests",
	label: "hosted OpenReleaseRequest",
};

/// Dispatch a GitHub Actions workflow through the monochange GitHub App.
pub(crate) const WORKFLOW_DISPATCH: HostedOperation = HostedOperation {
	rpc: "hosted_workflow_dispatch",
	legacy: "workflow-dispatches",
	label: "hosted workflow dispatch",
};

/// The RPC arguments: every hosted operation takes one `request`.
#[derive(Serialize)]
struct Arguments<'a, T> {
	request: &'a T,
}

/// The body an RPC operation answers a failure with.
#[derive(Deserialize)]
struct RpcError {
	status: u16,
	message: String,
}

/// Call `operation` on the app at `base_url`, signed with `token`.
///
/// `timeout` replaces the client's default for operations that wait on many
/// GitHub API calls.
pub(crate) async fn call<Request, Response>(
	operation: HostedOperation,
	base_url: &str,
	token: &str,
	request: &Request,
	timeout: Option<Duration>,
) -> MonochangeResult<Response>
where
	Request: Serialize,
	Response: DeserializeOwned,
{
	let base_url = base_url.trim_end_matches('/');
	let label = operation.label;
	let client = monochange_hosting::build_http_client("monochange app")?;
	let send = Send {
		client: &client,
		token,
		timeout,
		label,
	};

	let (mut status, mut text) = send
		.post(
			&format!("{base_url}/api/{}", operation.rpc),
			&Arguments { request },
		)
		.await?;
	let mut rpc_error = serde_json::from_str::<RpcError>(&text).ok();

	if status == StatusCode::NOT_FOUND && rpc_error.is_none() {
		(status, text) = send
			.post(&format!("{base_url}/api/{}", operation.legacy), request)
			.await?;
		rpc_error = None;
	}

	if !status.is_success() {
		let message = match rpc_error {
			Some(error) if error.status == status.as_u16() => error.message,
			_ => text,
		};
		return Err(MonochangeError::Config(format!(
			"{label} failed with HTTP {status}: {message}"
		)));
	}

	serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!("{label} response was invalid JSON: {error}"))
	})
}

/// What every attempt of one call shares.
struct Send<'a> {
	client: &'a reqwest::Client,
	token: &'a str,
	timeout: Option<Duration>,
	label: &'static str,
}

impl Send<'_> {
	/// POST `body` as JSON to `url` and read the answer.
	async fn post(
		&self,
		url: &str,
		body: &impl Serialize,
	) -> MonochangeResult<(StatusCode, String)> {
		let label = self.label;
		let mut builder = self
			.client
			.post(url)
			.bearer_auth(self.token)
			.header(reqwest::header::ACCEPT, "application/json")
			.json(body);
		if let Some(timeout) = self.timeout {
			builder = builder.timeout(timeout);
		}
		let response = builder
			.send()
			.await
			.map_err(|error| MonochangeError::Config(format!("{label} request failed: {error}")))?;
		let status = response.status();
		// patch-coverage:ignore-start -- a mid-body connection drop is needed to fail the text read; httpmock always serves complete bodies.
		let text = response.text().await.map_err(|error| {
			MonochangeError::Config(format!("{label} response read failed: {error}"))
		})?;
		// patch-coverage:ignore-end
		Ok((status, text))
	}
}
