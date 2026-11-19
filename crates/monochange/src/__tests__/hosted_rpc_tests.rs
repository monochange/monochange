//! Hosted operations go over RPC and fall back to REST on older deployments.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::time::Duration;

use httpmock::Method::POST;
use httpmock::MockServer;
use serde_json::Value;
use serde_json::json;

use super::RATE_LIMIT_RETRIES;
use super::RELEASE_COMMIT;
use super::RELEASE_REQUEST;
use super::RateLimitRetries;
use super::WORKFLOW_DISPATCH;
use super::call;
use super::call_with_retries;

fn request() -> Value {
	json!({ "owner": "acme", "repository": "actions" })
}

async fn commit(server: &MockServer) -> monochange_core::MonochangeResult<Value> {
	call(
		RELEASE_COMMIT,
		&format!("{}/", server.base_url()),
		"workflow-token",
		&request(),
		Some(std::time::Duration::from_secs(5)),
	)
	.await
}

#[tokio::test]
async fn operations_are_called_over_rpc_with_their_arguments() {
	let server = MockServer::start();
	let rpc = server.mock(|when, then| {
		when.method(POST)
			.path("/api/hosted_release_commit")
			.header("authorization", "Bearer workflow-token")
			.json_body(json!({ "request": request() }));
		then.status(200).json_body(json!({ "commit": "def456" }));
	});
	let legacy = server.mock(|when, then| {
		when.method(POST).path("/api/release-commits");
		then.status(200).json_body(json!({}));
	});

	assert_eq!(
		commit(&server).await.unwrap(),
		json!({ "commit": "def456" })
	);
	rpc.assert();
	legacy.assert_calls(0);
}

#[tokio::test]
async fn deployments_without_the_rpc_route_get_the_rest_request() {
	let server = MockServer::start();
	let rpc = server.mock(|when, then| {
		when.method(POST).path("/api/hosted_workflow_dispatch");
		then.status(404)
			.body("<!DOCTYPE html><title>Not found</title>");
	});
	let legacy = server.mock(|when, then| {
		when.method(POST)
			.path("/api/workflow-dispatches")
			.json_body(request());
		then.status(200)
			.json_body(json!({ "status": "dispatched" }));
	});

	let response: Value = call(
		WORKFLOW_DISPATCH,
		&server.base_url(),
		"workflow-token",
		&request(),
		None,
	)
	.await
	.unwrap();
	assert_eq!(response, json!({ "status": "dispatched" }));
	rpc.assert();
	legacy.assert();
}

#[tokio::test]
async fn rpc_failures_report_their_status_and_message() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_commit");
		then.status(409).json_body(json!({
			"status": 409,
			"message": "base branch `main` moved",
		}));
	});

	let error = commit(&server).await.unwrap_err().to_string();
	assert!(
		error.contains(
			"hosted CommitRelease failed with HTTP 409 Conflict: base branch `main` moved"
		),
		"{error}"
	);
}

#[tokio::test]
async fn an_rpc_not_found_is_an_answer_not_a_missing_route() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_request");
		then.status(404).json_body(json!({
			"status": 404,
			"message": "repository `acme/actions` has no monochange GitHub App installation",
		}));
	});
	let legacy = server.mock(|when, then| {
		when.method(POST).path("/api/release-requests");
		then.status(200).json_body(json!({}));
	});

	let error = call::<_, Value>(
		RELEASE_REQUEST,
		&server.base_url(),
		"workflow-token",
		&request(),
		None,
	)
	.await
	.unwrap_err()
	.to_string();
	assert!(
		error.contains("HTTP 404 Not Found: repository `acme/actions` has no monochange"),
		"{error}"
	);
	legacy.assert_calls(0);
}

#[tokio::test]
async fn bodies_that_disagree_with_the_status_are_reported_verbatim() {
	let server = MockServer::start();
	let body = json!({ "status": 409, "message": "stale" }).to_string();
	server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_commit");
		then.status(502).body(&body);
	});

	let error = commit(&server).await.unwrap_err().to_string();
	assert!(
		error.contains(&format!("HTTP 502 Bad Gateway: {body}")),
		"{error}"
	);
}

#[tokio::test]
async fn unreadable_answers_and_unreachable_servers_fail() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_commit");
		then.status(200).body("<html>not json</html>");
	});
	let error = commit(&server).await.unwrap_err().to_string();
	assert!(
		error.contains("hosted CommitRelease response was invalid JSON"),
		"{error}"
	);

	let error = call::<_, Value>(
		RELEASE_COMMIT,
		"http://127.0.0.1:9",
		"workflow-token",
		&request(),
		None,
	)
	.await
	.unwrap_err()
	.to_string();
	assert!(
		error.contains("hosted CommitRelease request failed"),
		"{error}"
	);
}

/// Retries that never wait long, so tests stay fast.
const QUICK_RETRIES: RateLimitRetries = RateLimitRetries {
	max_retries: 2,
	default_wait: Duration::ZERO,
	max_wait: Duration::from_secs(5),
};

async fn commit_with(
	server: &MockServer,
	retries: RateLimitRetries,
) -> monochange_core::MonochangeResult<Value> {
	call_with_retries(
		RELEASE_COMMIT,
		&server.base_url(),
		"workflow-token",
		&request(),
		Some(Duration::from_secs(5)),
		retries,
	)
	.await
}

#[test]
fn hosted_calls_wait_out_short_rate_limits_only() {
	assert_eq!(RATE_LIMIT_RETRIES.max_retries, 3);
	assert!(RATE_LIMIT_RETRIES.default_wait <= RATE_LIMIT_RETRIES.max_wait);
}

#[tokio::test]
async fn rate_limited_calls_are_sent_again_after_retry_after() {
	let server = MockServer::start_async().await;
	let limited = server
		.mock_async(|when, then| {
			when.method(POST).path("/api/hosted_release_commit");
			then.status(429)
				.header("retry-after", "1")
				.json_body(json!({ "status": 429, "message": "rate_limited" }));
		})
		.await;
	let pending = {
		let base_url = server.base_url();
		tokio::spawn(async move {
			call_with_retries::<_, Value>(
				RELEASE_COMMIT,
				&base_url,
				"workflow-token",
				&request(),
				None,
				QUICK_RETRIES,
			)
			.await
		})
	};

	// The call is now waiting out the one-second `retry-after`; let the
	// retry succeed.
	while limited.calls_async().await == 0 {
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	limited.delete_async().await;
	let succeeded = server
		.mock_async(|when, then| {
			when.method(POST).path("/api/hosted_release_commit");
			then.status(200).json_body(json!({ "commit": "def456" }));
		})
		.await;

	let response = pending.await.unwrap().unwrap();
	assert_eq!(response, json!({ "commit": "def456" }));
	succeeded.assert_calls_async(1).await;
}

#[tokio::test]
async fn calls_still_rate_limited_after_every_retry_fail_with_a_hint() {
	let server = MockServer::start();
	let limited = server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_commit");
		then.status(429)
			.json_body(json!({ "status": 429, "message": "rate_limited" }));
	});

	let error = commit_with(&server, QUICK_RETRIES)
		.await
		.unwrap_err()
		.to_string();
	limited.assert_calls(3);
	assert!(
		error.contains(
			"hosted CommitRelease failed with HTTP 429 Too Many Requests: rate_limited. The \
			 monochange app kept rate limiting this request after 2 retries; rerun the job in a \
			 few minutes"
		),
		"{error}"
	);
}

#[tokio::test]
async fn calls_fail_at_once_when_the_app_asks_for_too_long_a_wait() {
	let server = MockServer::start();
	let limited = server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_commit");
		then.status(429)
			.header("retry-after", "600")
			.body("slow down");
	});

	let error = commit_with(&server, QUICK_RETRIES)
		.await
		.unwrap_err()
		.to_string();
	limited.assert_calls(1);
	assert!(
		error.contains(
			"HTTP 429 Too Many Requests: slow down. The monochange app rate limited this \
			 request and asked for a 10m wait, longer than the 5s monochange waits; rerun the \
			 job after that"
		),
		"{error}"
	);
}

#[tokio::test]
async fn rest_fallbacks_are_retried_when_rate_limited_too() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(POST).path("/api/hosted_release_commit");
		then.status(404).body("not found");
	});
	let legacy = server.mock(|when, then| {
		when.method(POST).path("/api/release-commits");
		then.status(429).body("slow down");
	});

	let error = commit_with(&server, QUICK_RETRIES)
		.await
		.unwrap_err()
		.to_string();
	legacy.assert_calls(3);
	assert!(error.contains("after 2 retries"), "{error}");
}
