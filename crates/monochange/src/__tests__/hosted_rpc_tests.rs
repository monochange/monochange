//! Hosted operations go over RPC and fall back to REST on older deployments.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use httpmock::Method::POST;
use httpmock::MockServer;
use serde_json::Value;
use serde_json::json;

use super::RELEASE_COMMIT;
use super::RELEASE_REQUEST;
use super::WORKFLOW_DISPATCH;
use super::call;

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
