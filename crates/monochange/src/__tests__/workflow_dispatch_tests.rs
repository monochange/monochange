//! `monochange gh workflow run` parsing, request, and rendering.

#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;

use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use clap::Command;
use httpmock::Method;
use httpmock::MockServer;
use monochange_core::HostedCommitAuth;
use monochange_core::HostedWorkflowDispatchRequest;
use monochange_core::HostedWorkflowDispatchResponse;

use super::parse_fields;
use super::render_workflow_dispatch;
use super::send_workflow_dispatch;
use super::split_repository;
use crate::OutputFormat;
use crate::release_artifacts::resolve_hosted_commit_options;
use crate::tests::TEST_ENV_LOCK;

fn field_matches(fields: &[&str]) -> ArgMatches {
	Command::new("test")
		.arg(Arg::new("field").long("field").action(ArgAction::Append))
		.try_get_matches_from(
			std::iter::once("test").chain(fields.iter().flat_map(|field| ["--field", *field])),
		)
		.unwrap()
}

#[test]
fn repository_requires_owner_and_name() {
	assert_eq!(
		split_repository("monochange/actions").unwrap(),
		("monochange".to_string(), "actions".to_string())
	);

	for repository in [
		"actions",
		"monochange/actions/extra",
		"/actions",
		"monochange/",
	] {
		assert!(
			split_repository(repository).is_err(),
			"accepted {repository}"
		);
	}
}

#[test]
fn fields_require_key_value_pairs() {
	let fields = parse_fields(&field_matches(&["pull_request=103", "comment=always"])).unwrap();
	assert_eq!(
		fields,
		BTreeMap::from([
			("comment".to_string(), "always".to_string()),
			("pull_request".to_string(), "103".to_string()),
		])
	);

	assert!(parse_fields(&field_matches(&["missing-separator"])).is_err());
	assert!(parse_fields(&field_matches(&["=value"])).is_err());
	assert!(parse_fields(&field_matches(&["key=one", "key=two"])).is_err());
}

fn request() -> HostedWorkflowDispatchRequest {
	HostedWorkflowDispatchRequest {
		provider: "github".to_string(),
		owner: "monochange".to_string(),
		repository: "actions".to_string(),
		workflow: "release.yml".to_string(),
		git_ref: "main".to_string(),
		inputs: BTreeMap::from([("pull_request".to_string(), "103".to_string())]),
		dry_run: false,
	}
}

fn response() -> HostedWorkflowDispatchResponse {
	HostedWorkflowDispatchResponse {
		status: Some("dispatched".to_string()),
		workflow: Some("release.yml".to_string()),
		git_ref: Some("main".to_string()),
		message: None,
	}
}

#[test]
fn text_and_markdown_outputs_name_the_target() {
	let text = render_workflow_dispatch(&request(), &response(), OutputFormat::Text).unwrap();
	assert_eq!(
		text,
		"Dispatched monochange/actions release.yml on main via monochange app"
	);

	let markdown =
		render_workflow_dispatch(&request(), &response(), OutputFormat::Markdown).unwrap();
	assert!(markdown.contains("| `monochange/actions` | `release.yml` | `main` | `dispatched` |"));
}

#[test]
fn dry_run_text_says_would_dispatch() {
	let mut request = request();
	request.dry_run = true;
	let response = HostedWorkflowDispatchResponse {
		status: Some("dry_run".to_string()),
		..response()
	};

	let text = render_workflow_dispatch(&request, &response, OutputFormat::Text).unwrap();
	assert_eq!(
		text,
		"Would dispatch monochange/actions release.yml on main via monochange app"
	);
}

#[test]
fn json_output_serializes_the_server_response() {
	let output = render_workflow_dispatch(&request(), &response(), OutputFormat::Json).unwrap();
	let value: serde_json::Value = serde_json::from_str(&output).unwrap();
	assert_eq!(value["status"], "dispatched");
	assert_eq!(value["workflow"], "release.yml");
	assert_eq!(value["ref"], "main");
}

#[tokio::test]
async fn send_posts_the_expected_request_and_parses_the_response() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/api/workflow-dispatches")
			.header("authorization", "Bearer monochange-token")
			.json_body(serde_json::json!({
				"provider": "github",
				"owner": "monochange",
				"repository": "actions",
				"workflow": "release.yml",
				"ref": "main",
				"inputs": {"pull_request": "103"},
				"dry_run": false
			}));
		then.status(200).json_body(serde_json::json!({
			"status": "dispatched",
			"workflow": "release.yml",
			"ref": "main"
		}));
	});
	let options =
		resolve_hosted_commit_options(HostedCommitAuth::Token, Some(&server.base_url()), None);

	let response = send_workflow_dispatch(&request(), &options, "monochange-token")
		.await
		.unwrap();

	assert_eq!(response, response_fixture());
	mock.assert();
}

fn response_fixture() -> HostedWorkflowDispatchResponse {
	HostedWorkflowDispatchResponse {
		status: Some("dispatched".to_string()),
		workflow: Some("release.yml".to_string()),
		git_ref: Some("main".to_string()),
		message: None,
	}
}

#[test]
fn run_dispatches_through_the_token_backend() {
	use std::sync::PoisonError;

	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/api/workflow-dispatches")
			.header("authorization", "Bearer monochange-token")
			.json_body(serde_json::json!({
				"provider": "github",
				"owner": "monochange",
				"repository": "actions",
				"workflow": "release.yml",
				"ref": "main",
				"dry_run": true
			}));
		then.status(200).json_body(serde_json::json!({
			"status": "dry_run",
			"workflow": "release.yml",
			"ref": "main"
		}));
	});
	let command = crate::build_command("monochange");
	let matches = command
		.try_get_matches_from([
			"monochange",
			"gh",
			"workflow",
			"run",
			"release.yml",
			"--repo",
			"monochange/actions",
			"--ref",
			"main",
			"--dry-run",
			"--hosted-auth",
			"token",
			"--hosted-url",
			&server.base_url(),
			"--format",
			"json",
		])
		.unwrap();
	let run_matches = matches
		.subcommand_matches("gh")
		.and_then(|matches| matches.subcommand_matches("workflow"))
		.and_then(|matches| matches.subcommand_matches("run"))
		.expect("gh workflow run matches");

	let _env_lock = TEST_ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
	crate::tests::block_on_in_context(temp_env::async_with_vars(
		[("MONOCHANGE_TOKEN", Some("monochange-token"))],
		async {
			let output = super::run(run_matches)
				.await
				.unwrap_or_else(|error| panic!("dispatch: {error}"));
			let value: serde_json::Value = serde_json::from_str(&output).unwrap();
			assert_eq!(value["status"], "dry_run");
		},
	));
	mock.assert();
}

#[tokio::test]
async fn send_reports_http_and_json_failures() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::POST).path("/api/workflow-dispatches");
		then.status(403).body("workflow is not connected");
	});
	let options =
		resolve_hosted_commit_options(HostedCommitAuth::Token, Some(&server.base_url()), None);

	let error = send_workflow_dispatch(&request(), &options, "monochange-token")
		.await
		.expect_err("HTTP failure must surface");
	assert!(error.to_string().contains("HTTP 403 Forbidden"), "{error}");

	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::POST).path("/api/workflow-dispatches");
		then.status(200).body("not json");
	});
	let options =
		resolve_hosted_commit_options(HostedCommitAuth::Token, Some(&server.base_url()), None);

	let error = send_workflow_dispatch(&request(), &options, "monochange-token")
		.await
		.expect_err("invalid JSON must surface");
	assert!(
		error.to_string().contains("response was invalid JSON"),
		"{error}"
	);
}
