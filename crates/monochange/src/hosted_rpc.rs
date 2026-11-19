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
//!
//! A `429` means the app's rate limit refused the request before running it,
//! so the request is sent again after the app's `Retry-After`. Each wait is
//! announced as a warning, so the command says it is waiting instead of
//! looking hung.

#[cfg(test)]
#[path = "__tests__/hosted_rpc_tests.rs"]
mod tests;

use std::time::Duration;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_hosting::retry::RetryNotice;
use monochange_hosting::retry::RetryReason;
use monochange_hosting::retry::format_wait;
use monochange_hosting::retry::retry_after;
use monochange_hosting::retry::warn_retry;
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

/// How [`call`] retries a request the monochange app rate limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RateLimitRetries {
	/// How many times a rate-limited request is sent again.
	pub(crate) max_retries: usize,
	/// The wait when the app's `429` carries no `Retry-After`.
	pub(crate) default_wait: Duration,
	/// The longest wait monochange accepts. A `Retry-After` further out
	/// fails the call at once, naming the wait, instead of sleeping through
	/// most of a CI job.
	pub(crate) max_wait: Duration,
}

/// The retries every hosted call makes.
pub(crate) const RATE_LIMIT_RETRIES: RateLimitRetries = RateLimitRetries {
	max_retries: 3,
	default_wait: Duration::from_secs(30),
	max_wait: Duration::from_secs(5 * 60),
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
	call_with_retries(
		operation,
		base_url,
		token,
		request,
		timeout,
		RATE_LIMIT_RETRIES,
	)
	.await
}

/// [`call`], with the rate-limit retries given.
pub(crate) async fn call_with_retries<Request, Response>(
	operation: HostedOperation,
	base_url: &str,
	token: &str,
	request: &Request,
	timeout: Option<Duration>,
	retries: RateLimitRetries,
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
		retries,
	};

	let mut answer = send
		.post(
			&format!("{base_url}/api/{}", operation.rpc),
			&Arguments { request },
		)
		.await?;
	let mut rpc_error = serde_json::from_str::<RpcError>(&answer.text).ok();

	if answer.status == StatusCode::NOT_FOUND && rpc_error.is_none() {
		answer = send
			.post(&format!("{base_url}/api/{}", operation.legacy), request)
			.await?;
		rpc_error = None;
	}

	let Answer {
		status,
		text,
		retry_after,
	} = answer;

	if !status.is_success() {
		let message = match rpc_error {
			Some(error) if error.status == status.as_u16() => error.message,
			_ => text,
		};
		let hint = if status == StatusCode::TOO_MANY_REQUESTS {
			rate_limit_hint(retry_after, retries)
		} else {
			String::new()
		};
		return Err(MonochangeError::Config(format!(
			"{label} failed with HTTP {status}: {message}{hint}"
		)));
	}

	serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!("{label} response was invalid JSON: {error}"))
	})
}

/// Why a rate-limited call gave up, for its error message.
fn rate_limit_hint(retry_after: Option<Duration>, retries: RateLimitRetries) -> String {
	match retry_after {
		Some(wait) if wait > retries.max_wait => {
			format!(
				". The monochange app rate limited this request and asked for a {} wait, longer than the {} monochange waits; rerun the job after that",
				format_wait(wait),
				format_wait(retries.max_wait)
			)
		}
		_ => {
			format!(
				". The monochange app kept rate limiting this request after {} retries; rerun the job in a few minutes",
				retries.max_retries
			)
		}
	}
}

/// What every attempt of one call shares.
struct Send<'a> {
	client: &'a reqwest::Client,
	token: &'a str,
	timeout: Option<Duration>,
	label: &'static str,
	retries: RateLimitRetries,
}

/// The app's answer to one request.
struct Answer {
	status: StatusCode,
	text: String,
	/// The wait a `Retry-After` header asked for.
	retry_after: Option<Duration>,
}

impl Send<'_> {
	/// POST `body` as JSON to `url` and read the answer, sending it again
	/// while the app rate limits it.
	async fn post(&self, url: &str, body: &impl Serialize) -> MonochangeResult<Answer> {
		let retries = self.retries;

		for retry in 1..=retries.max_retries {
			let answer = self.post_once(url, body).await?;
			if answer.status != StatusCode::TOO_MANY_REQUESTS {
				return Ok(answer);
			}
			let wait = answer.retry_after.unwrap_or(retries.default_wait);
			if wait > retries.max_wait {
				return Ok(answer);
			}
			warn_retry(&RetryNotice {
				service: "monochange app",
				operation: self.label,
				reason: RetryReason::RateLimited,
				wait,
				retry,
				max_retries: retries.max_retries,
			});
			tokio::time::sleep(wait).await;
		}

		self.post_once(url, body).await
	}

	/// POST `body` as JSON to `url` once and read the answer.
	async fn post_once(&self, url: &str, body: &impl Serialize) -> MonochangeResult<Answer> {
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
		let retry_after = retry_after(response.headers());
		// patch-coverage:ignore-start -- a mid-body connection drop is needed to fail the text read; httpmock always serves complete bodies.
		let text = response.text().await.map_err(|error| {
			MonochangeError::Config(format!("{label} response read failed: {error}"))
		})?;
		// patch-coverage:ignore-end
		Ok(Answer {
			status,
			text,
			retry_after,
		})
	}
}
