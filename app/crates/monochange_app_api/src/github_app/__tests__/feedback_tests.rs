// Tokio's test macro and the HTTP mock harness construct runtimes with expect.
#![allow(clippy::disallowed_methods)]

use httpmock::Method::GET;
use httpmock::Method::POST;
use httpmock::MockServer;

use super::*;

fn files() -> Vec<FeedbackFile> {
	vec![
		FeedbackFile {
			path: "src/run.sh".into(),
			content: Some("exit 0".into()),
			executable: true,
		},
		FeedbackFile {
			path: "src/old.js".into(),
			content: None,
			executable: false,
		},
	]
}
fn request(files: &[FeedbackFile]) -> FeedbackPullRequest<'_> {
	FeedbackPullRequest {
		repository: "acme/web",
		base: "base",
		branch: "monochange/feedback/1/job-1",
		title: "feat: export CSV",
		body: "Feedback-Item: fb-1",
		marker: "Feedback-Result: digest",
		files,
	}
}
async fn setup(server: &MockServer) {
	server
		.mock_async(|when, then| {
			when.method(GET).path("/repos/acme/web");
			then.status(200).json_body(json!({"default_branch":"main"}));
		})
		.await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/repos/acme/web/git/commits/base");
			then.status(200)
				.json_body(json!({"tree":{"sha":"base-tree"}}));
		})
		.await;
	server
		.mock_async(|when, then| {
			when.method(POST)
				.path("/repos/acme/web/git/trees")
				.body_includes("100755")
				.body_includes("\"sha\":null");
			then.status(201).json_body(json!({"sha":"result-tree"}));
		})
		.await;
}

#[tokio::test]
async fn only_matching_persisted_commits_and_pull_requests_are_reused() {
	for scenario in [
		"match",
		"tree",
		"parent",
		"marker",
		"head",
		"base",
		"body",
		"branch-error",
		"pulls-error",
	] {
		let server = MockServer::start_async().await;
		setup(&server).await;
		server
			.mock_async(|when, then| {
				when.method(GET)
					.path("/repos/acme/web/git/ref/heads/monochange/feedback/1/job-1");
				then.status(if scenario == "branch-error" { 500 } else { 200 })
					.json_body(json!({"object":{"sha":"commit"}}));
			})
			.await;

		server.mock_async(|when,then| {when.method(GET).path("/repos/acme/web/git/commits/commit");then.status(200).json_body(json!({"tree":{"sha":if scenario=="tree" {"unrelated"} else {"result-tree"}},"parents":[{"sha":if scenario=="parent" {"unrelated"} else {"base"}}],"message":if scenario=="marker" {"unrelated"} else {"feat: export CSV\n\nFeedback-Result: digest"}}));}).await;

		server.mock_async(|when,then| {when.method(GET).path("/repos/acme/web/pulls");then.status(if scenario=="pulls-error" {500} else {200}).json_body(json!([{"number":42,"html_url":"https://github.com/acme/web/pull/42","head":{"sha":if scenario=="head" {"unrelated"} else {"commit"},"ref":"monochange/feedback/1/job-1"},"base":{"ref":if scenario=="base" {"other"} else {"main"}},"body":if scenario=="body" {"unrelated"} else {"Feedback-Item: fb-1\n\nFeedback-Result: digest"}}]));}).await;
		let files = files();
		let value = create_feedback_pull_request(
			&reqwest::Client::new(),
			&server.base_url(),
			"test-token",
			&request(&files),
		)
		.await;

		if scenario == "match" {
			assert_eq!(
				value.unwrap(),
				(42, "https://github.com/acme/web/pull/42".into())
			);
		} else {
			assert!(value.is_err(), "{scenario}");
		}
	}
}

#[tokio::test]
async fn unverified_commits_never_create_a_branch() {
	let server = MockServer::start_async().await;
	setup(&server).await;
	server
		.mock_async(|when, then| {
			when.method(GET)
				.path("/repos/acme/web/git/ref/heads/monochange/feedback/1/job-1");
			then.status(404);
		})
		.await;
	server
		.mock_async(|when, then| {
			when.method(POST).path("/repos/acme/web/git/commits");
			then.status(201)
				.json_body(json!({"sha":"unverified","verification":{"verified":false}}));
		})
		.await;
	let writes = server
		.mock_async(|when, then| {
			when.method(POST).path("/repos/acme/web/git/refs");
			then.status(201).json_body(json!({}));
		})
		.await;
	let files = files();
	assert!(matches!(
		create_feedback_pull_request(
			&reqwest::Client::new(),
			&server.base_url(),
			"test-token",
			&request(&files)
		)
		.await,
		Err(GitHubAppError::UnverifiedCommit(_, _))
	));
	writes.assert_calls_async(0).await;
}

#[tokio::test]
async fn malformed_or_failed_provider_responses_never_authorize_publication() {
	let files = files();
	let mut input = request(&files);
	input.branch = "main";
	assert!(
		create_feedback_pull_request(
			&reqwest::Client::new(),
			"http://localhost:1",
			"test-token",
			&input
		)
		.await
		.is_err()
	);
	input = request(&[]);
	assert!(
		create_feedback_pull_request(
			&reqwest::Client::new(),
			"http://localhost:1",
			"test-token",
			&input
		)
		.await
		.is_err()
	);

	for status in [200, 503] {
		let server = MockServer::start_async().await;

		server
			.mock_async(|when, then| {
				when.method(GET).path("/repos/acme/web");
				then.status(status).json_body(json!({}));
			})
			.await;
		assert!(
			create_feedback_pull_request(
				&reqwest::Client::new(),
				&server.base_url(),
				"test-token",
				&request(&files)
			)
			.await
			.is_err()
		);
	}
}

#[tokio::test]
async fn incomplete_tree_and_branch_identifiers_are_not_reused() {
	for broken_tree in [true, false] {
		let server = MockServer::start_async().await;
		server
			.mock_async(|when, then| {
				when.method(GET).path("/repos/acme/web");
				then.status(200).json_body(json!({"default_branch":"main"}));
			})
			.await;
		server
			.mock_async(|when, then| {
				when.method(GET).path("/repos/acme/web/git/commits/base");
				then.status(200).json_body(if broken_tree {
					json!({"tree":{}})
				} else {
					json!({"tree":{"sha":"base-tree"}})
				});
			})
			.await;
		server
			.mock_async(|when, then| {
				when.method(POST).path("/repos/acme/web/git/trees");
				then.status(201).json_body(json!({"sha":"result-tree"}));
			})
			.await;
		server
			.mock_async(|when, then| {
				when.method(GET)
					.path("/repos/acme/web/git/ref/heads/monochange/feedback/1/job-1");
				then.status(200).json_body(json!({"object":{}}));
			})
			.await;
		let files = files();
		assert!(
			create_feedback_pull_request(
				&reqwest::Client::new(),
				&server.base_url(),
				"test-token",
				&request(&files)
			)
			.await
			.is_err()
		);
	}
}
