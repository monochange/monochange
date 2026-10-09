//! Authenticated app identity determines the repository connection link.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::OnceLock;

use httpmock::Method;
use httpmock::Mock;
use httpmock::MockServer;
use monochange_core::HostedCommitFile;
use monochange_core::HostedCommitRequest;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;
use rstest::rstest;

use super::GitHubAppAuth;
use super::GitHubAppError;
use super::create_release_commit;

fn auth(server: &MockServer) -> GitHubAppAuth {
	static TEST_KEY: OnceLock<String> = OnceLock::new();
	let pem = TEST_KEY.get_or_init(|| {
		RsaPrivateKey::new(&mut OsRng, 2048)
			.unwrap()
			.to_pkcs1_pem(LineEnding::LF)
			.unwrap()
			.to_string()
	});
	GitHubAppAuth::new("123", pem, "test-only-webhook-secret", &server.base_url())
}

#[tokio::test]
async fn installation_link_uses_authenticated_app_slug_and_fixed_github_origin() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/app")
			.header_exists("authorization")
			.header("accept", "application/vnd.github+json")
			.header("user-agent", "monochange");
		then.status(200).json_body(serde_json::json!({
			"slug": "monochange-test-app",
			"html_url": "https://untrusted.example/login",
		}));
	});

	assert_eq!(
		auth(&server)
			.installation_url(&reqwest::Client::new())
			.await
			.unwrap(),
		"https://github.com/apps/monochange-test-app/installations/new",
	);
	metadata.assert();
}

#[tokio::test]
async fn failed_app_lookup_preserves_status_instead_of_inventing_an_installation_link() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(401)
			.json_body(serde_json::json!({"message": "bad credentials"}));
	});

	assert!(matches!(
		auth(&server)
			.installation_url(&reqwest::Client::new())
			.await,
		Err(GitHubAppError::Status("read app metadata", status, _))
			if status == reqwest::StatusCode::UNAUTHORIZED
	));
	metadata.assert();
}

#[rstest]
#[case("")]
#[case("../another-app")]
#[case("valid?redirect=https://untrusted.example")]
#[case("valid#fragment")]
#[case("valid%2fanother")]
#[case("valid_app")]
#[case(" spaced ")]
#[tokio::test]
async fn invalid_app_slug_cannot_change_the_installation_url(#[case] slug: &str) {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200)
			.json_body(serde_json::json!({"slug": slug}));
	});

	assert!(matches!(
		auth(&server)
			.installation_url(&reqwest::Client::new())
			.await,
		Err(GitHubAppError::InvalidAppSlug)
	));
	metadata.assert();
}

#[tokio::test]
async fn malformed_metadata_is_an_error() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200).json_body(serde_json::json!({"id": 123}));
	});

	assert!(matches!(
		auth(&server)
			.installation_url(&reqwest::Client::new())
			.await,
		Err(GitHubAppError::Http(_))
	));
	metadata.assert();
}

#[tokio::test]
async fn invalid_signing_key_fails_before_requesting_github() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200)
			.json_body(serde_json::json!({"slug": "test-app"}));
	});
	let app = GitHubAppAuth::new(
		"123",
		"invalid-test-key",
		"test-only-secret",
		&server.base_url(),
	);

	assert!(matches!(
		app.installation_url(&reqwest::Client::new()).await,
		Err(GitHubAppError::Jwt(_))
	));
	metadata.assert_calls(0);
}

// ── Hosted release commits ──

const RELEASE_BRANCH: &str = "monochange/release/release";
const BASE_COMMIT: &str = "base-sha";
const RELEASE_COMMIT: &str = "release-sha";

fn release_commit_request(branch: &str, base_branch: Option<&str>) -> HostedCommitRequest {
	HostedCommitRequest {
		provider: "github".to_string(),
		owner: "acme".to_string(),
		repository: "actions".to_string(),
		branch: branch.to_string(),
		base_branch: base_branch.map(String::from),
		base_commit: BASE_COMMIT.to_string(),
		subject: "chore(release): prepare release".to_string(),
		body: String::new(),
		files: vec![
			HostedCommitFile {
				path: ".monochange/releases/abc123/release.json".to_string(),
				content: Some("{}".to_string()),
			},
			HostedCommitFile {
				path: ".changeset/feature.md".to_string(),
				content: None,
			},
		],
		dry_run: false,
		idempotency_key: None,
	}
}

/// Serve a branch ref at `sha`, or a 404 when the branch does not exist.
fn mock_branch<'a>(server: &'a MockServer, branch: &str, sha: Option<&str>) -> Mock<'a> {
	let path = format!("/repos/acme/actions/git/ref/heads/{branch}");
	server.mock(|when, then| {
		when.method(Method::GET).path(path);
		match sha {
			Some(sha) => {
				then.status(200)
					.json_body(serde_json::json!({ "object": { "sha": sha } }));
			}
			None => {
				then.status(404);
			}
		}
	})
}

/// Serve the repository metadata with `default_branch`.
fn mock_repository<'a>(server: &'a MockServer, default_branch: &str) -> Mock<'a> {
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions");
		then.status(200)
			.json_body(serde_json::json!({ "default_branch": default_branch }));
	})
}

/// The Git Database writes that create the release commit.
struct CommitWrites<'a> {
	blob: Mock<'a>,
	tree: Mock<'a>,
	commit: Mock<'a>,
}

fn mock_commit_writes(server: &MockServer) -> CommitWrites<'_> {
	CommitWrites {
		blob: server.mock(|when, then| {
			when.method(Method::POST)
				.path("/repos/acme/actions/git/blobs");
			then.status(201)
				.json_body(serde_json::json!({ "sha": "blob-sha" }));
		}),
		tree: server.mock(|when, then| {
			when.method(Method::POST)
				.path("/repos/acme/actions/git/trees")
				.json_body_includes(r#"{ "base_tree": "base-sha" }"#);
			then.status(201)
				.json_body(serde_json::json!({ "sha": "tree-sha" }));
		}),
		commit: server.mock(|when, then| {
			when.method(Method::POST)
				.path("/repos/acme/actions/git/commits")
				.json_body_includes(r#"{ "tree": "tree-sha", "parents": ["base-sha"] }"#);
			then.status(201).json_body(serde_json::json!({
				"sha": RELEASE_COMMIT,
				"verification": { "verified": true, "reason": "valid" },
			}));
		}),
	}
}

/// Accept a ref update that moves `branch` to the release commit.
fn mock_branch_update<'a>(server: &'a MockServer, branch: &str, force: bool) -> Mock<'a> {
	let path = format!("/repos/acme/actions/git/refs/heads/{branch}");
	server.mock(|when, then| {
		when.method(Method::PATCH)
			.path(path)
			.json_body(serde_json::json!({ "sha": RELEASE_COMMIT, "force": force }));
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": RELEASE_COMMIT } }));
	})
}

async fn create_commit(
	server: &MockServer,
	request: &HostedCommitRequest,
) -> Result<(String, bool, Option<String>), GitHubAppError> {
	create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"installation-token",
		request,
	)
	.await
}

#[tokio::test]
async fn first_release_commit_creates_the_release_branch() {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, None);
	let writes = mock_commit_writes(&server);
	let create = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/refs")
			.json_body(serde_json::json!({
				"ref": "refs/heads/monochange/release/release",
				"sha": RELEASE_COMMIT,
			}));
		then.status(201);
	});

	let result = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap();

	assert_eq!(
		result,
		(RELEASE_COMMIT.to_string(), true, Some("valid".to_string()))
	);
	writes.blob.assert_calls(1);
	writes.tree.assert();
	writes.commit.assert();
	create.assert();
}

#[tokio::test]
async fn release_branch_at_the_base_commit_is_fast_forwarded() {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some(BASE_COMMIT));
	mock_commit_writes(&server);
	let update = mock_branch_update(&server, RELEASE_BRANCH, false);

	create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap();

	update.assert();
}

#[tokio::test]
async fn existing_release_branch_is_regenerated_from_the_current_base() {
	// Every push to the base branch prepares the release again; the previous
	// release commit is replaced, not built upon.
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some("previous-release-sha"));
	let base = mock_branch(&server, "main", Some(BASE_COMMIT));
	let repository = mock_repository(&server, "main");
	mock_commit_writes(&server);
	let update = mock_branch_update(&server, RELEASE_BRANCH, true);

	let (commit, verified, _) = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap();

	assert_eq!(commit, RELEASE_COMMIT);
	assert!(verified);
	base.assert();
	repository.assert();
	update.assert();
}

#[tokio::test]
async fn stale_run_cannot_replace_the_release_branch() {
	// A newer push moved the base branch, so a newer run owns the release.
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some("newer-release-sha"));
	mock_branch(&server, "main", Some("newer-base-sha"));
	let writes = mock_commit_writes(&server);
	let update = mock_branch_update(&server, RELEASE_BRANCH, true);

	let error = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::BaseMoved(branch, actual, expected)
				if branch == "main" && actual == "newer-base-sha" && expected == BASE_COMMIT
		),
		"{error}"
	);
	writes.blob.assert_calls(0);
	writes.tree.assert_calls(0);
	writes.commit.assert_calls(0);
	update.assert_calls(0);
}

#[tokio::test]
async fn missing_base_branch_cannot_replace_the_release_branch() {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some("previous-release-sha"));
	mock_branch(&server, "main", None);
	let update = mock_branch_update(&server, RELEASE_BRANCH, true);

	let error = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::Status("read base branch ref", status, _)
				if *status == reqwest::StatusCode::NOT_FOUND
		),
		"{error}"
	);
	update.assert_calls(0);
}

#[rstest]
#[case::client_without_base_branch(RELEASE_BRANCH, None)]
#[case::base_branch_as_release_branch("main", Some("main"))]
#[tokio::test]
async fn release_branch_that_moved_is_never_force_updated(
	#[case] branch: &str,
	#[case] base_branch: Option<&str>,
) {
	let server = MockServer::start();
	mock_branch(&server, branch, Some("other-sha"));
	let update = mock_branch_update(&server, branch, true);

	let error = create_commit(&server, &release_commit_request(branch, base_branch))
		.await
		.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::BranchMoved(moved, actual, expected)
				if moved == branch && actual == "other-sha" && expected == BASE_COMMIT
		),
		"{error}"
	);
	update.assert_calls(0);
}

#[tokio::test]
async fn unreadable_release_branch_fails_before_writing() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/ref/heads/monochange/release/release");
		then.status(500).body("unavailable");
	});
	let writes = mock_commit_writes(&server);

	let error = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::Status("read release branch ref", status, body)
				if *status == reqwest::StatusCode::INTERNAL_SERVER_ERROR && body == "unavailable"
		),
		"{error}"
	);
	writes.blob.assert_calls(0);
}

#[tokio::test]
async fn rejected_fast_forward_reports_the_ref_update() {
	// GitHub refuses a non-forced update when the branch moved after the check.
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some(BASE_COMMIT));
	mock_commit_writes(&server);
	server.mock(|when, then| {
		when.method(Method::PATCH)
			.path("/repos/acme/actions/git/refs/heads/monochange/release/release");
		then.status(422).body("Update is not a fast forward");
	});

	let error = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::Status("update release branch ref", status, _)
				if *status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
		),
		"{error}"
	);
}

#[rstest]
#[case::not_a_release_branch("feature/work", "main")]
#[case::bare_release_name("release", "main")]
#[case::release_branch_is_the_default_branch(RELEASE_BRANCH, RELEASE_BRANCH)]
#[tokio::test]
async fn only_release_branches_that_are_not_the_default_branch_are_forced(
	#[case] branch: &str,
	#[case] default_branch: &str,
) {
	// A caller must not be able to name `main` (or any non-release branch)
	// and have it forced onto another branch's commit.
	let server = MockServer::start();
	mock_branch(&server, branch, Some("other-sha"));
	mock_branch(&server, "main", Some(BASE_COMMIT));
	mock_repository(&server, default_branch);
	let writes = mock_commit_writes(&server);
	let update = mock_branch_update(&server, branch, true);

	let error = create_commit(&server, &release_commit_request(branch, Some("main")))
		.await
		.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::BranchMoved(moved, actual, expected)
				if moved == branch && actual == "other-sha" && expected == BASE_COMMIT
		),
		"{error}"
	);
	writes.blob.assert_calls(0);
	update.assert_calls(0);
}

#[tokio::test]
async fn unreadable_repository_metadata_refuses_to_force() {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some("previous-release-sha"));
	mock_branch(&server, "main", Some(BASE_COMMIT));
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions");
		then.status(403).body("forbidden");
	});
	let update = mock_branch_update(&server, RELEASE_BRANCH, true);

	let error = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::Status("read repository", status, _)
				if *status == reqwest::StatusCode::FORBIDDEN
		),
		"{error}"
	);
	update.assert_calls(0);
}
