//! Tests for the monochange GitHub App identity and release-commit creation.
//!
//! The authenticated app identity determines the repository connection link,
//! and the hosted release-commit contract — including the hard failure when
//! GitHub does not verify a commit — stays pinned to observable behavior.

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
use super::PendingPermission;
use super::PermissionLevels;
use super::create_release_commit;
use super::pending_permissions;

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

#[tokio::test]
async fn installation_account_recovers_the_stable_account_identity() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/app/installations/1001")
			.header_exists("authorization");
		then.status(200).json_body(serde_json::json!({
			"id": 1001,
			"account": {"id": 5001, "login": "acme", "type": "Organization", "avatar_url": "https://avatars/acme.png"}
		}));
	});
	let account = auth(&server)
		.installation_account(&reqwest::Client::new(), 1001)
		.await
		.unwrap();
	lookup.assert();
	assert_eq!(account.external_id, 5001);
	assert_eq!(account.login, "acme");
	assert_eq!(account.account_type, "Organization");
	assert_eq!(
		account.avatar_url.as_deref(),
		Some("https://avatars/acme.png")
	);

	assert_eq!(account.provider, "github");
}

#[tokio::test]
async fn installation_account_failures_keep_github_status() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::GET).path("/app/installations/404");
		then.status(404).body("Not Found");
	});
	let error = auth(&server)
		.installation_account(&reqwest::Client::new(), 404)
		.await
		.unwrap_err();
	assert!(matches!(
		error,
		GitHubAppError::Status("read installation", status, _) if status == reqwest::StatusCode::NOT_FOUND
	));
}

// ── Permission updates ──
fn levels(entries: &[(&str, &str)]) -> PermissionLevels {
	entries
		.iter()
		.map(|(name, level)| ((*name).to_string(), (*level).to_string()))
		.collect()
}

#[tokio::test]
async fn requested_and_installation_permissions_come_from_github() {
	let server = MockServer::start();
	let app = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/app")
			.header_exists("authorization");
		then.status(200).json_body(serde_json::json!({
			"slug": "monochange",
			"permissions": {"actions": "write", "contents": "write", "metadata": "read"},
		}));
	});
	let installation = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/app/installations/1001")
			.header_exists("authorization");
		then.status(200).json_body(serde_json::json!({
			"id": 1001,
			"account": {"id": 5001, "login": "acme", "type": "Organization"},
			"permissions": {"contents": "write", "metadata": "read"},
		}));
	});
	let auth = auth(&server);
	let http = reqwest::Client::new();

	assert_eq!(
		auth.requested_permissions(&http).await.unwrap(),
		levels(&[
			("actions", "write"),
			("contents", "write"),
			("metadata", "read")
		]),
	);
	assert_eq!(
		auth.installation_permissions(&http, 1001).await.unwrap(),
		levels(&[("contents", "write"), ("metadata", "read")]),
	);
	app.assert();
	installation.assert();
}

#[tokio::test]
async fn installation_permission_failures_keep_github_status() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::GET).path("/app/installations/404");
		then.status(404).body("Not Found");
	});

	assert!(matches!(
		auth(&server)
			.installation_permissions(&reqwest::Client::new(), 404)
			.await,
		Err(GitHubAppError::Status("read installation", status, _))
			if status == reqwest::StatusCode::NOT_FOUND
	));
}

#[test]
fn pending_permissions_lists_new_and_raised_access_only() {
	let requested = levels(&[
		("actions", "write"),
		("checks", "read"),
		("contents", "write"),
		("issues", "write"),
		("metadata", "read"),
		("future", "someday"),
	]);
	let granted = levels(&[
		("checks", "write"),
		("contents", "write"),
		("issues", "read"),
		("metadata", "read"),
	]);

	assert_eq!(
		pending_permissions(&requested, &granted),
		vec![
			PendingPermission {
				name: "actions".to_string(),
				requested: "write".to_string(),
				granted: None,
			},
			PendingPermission {
				name: "issues".to_string(),
				requested: "write".to_string(),
				granted: Some("read".to_string()),
			},
		],
	);
	assert!(
		pending_permissions(
			&levels(&[("administration", "admin")]),
			&levels(&[("administration", "admin")])
		)
		.is_empty()
	);
}

// ── Hosted release commits ──
const RELEASE_BRANCH: &str = "monochange/release/release";
const BASE_COMMIT: &str = "base-sha";
const RELEASE_COMMIT: &str = "release-sha";

#[rstest]
#[case::create(None)]
#[case::fast_forward(Some(BASE_COMMIT))]
#[tokio::test]
async fn default_release_branch_is_rejected_before_every_write(
	#[case] head: Option<&str>,
	#[values(Some("main"), None)] base: Option<&str>,
) {
	let server = MockServer::start();
	mock_repository(&server, RELEASE_BRANCH);
	mock_branch(&server, RELEASE_BRANCH, head);
	let writes = mock_commit_writes(&server);
	let create_ref = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/refs");
		then.status(201);
	});
	let update_ref = mock_branch_update(&server, RELEASE_BRANCH, false);

	assert!(
		create_commit(&server, &release_commit_request(RELEASE_BRANCH, base))
			.await
			.is_err()
	);
	writes.blob.assert_calls(0);
	writes.tree.assert_calls(0);
	writes.commit.assert_calls(0);
	create_ref.assert_calls(0);
	update_ref.assert_calls(0);
}

#[rstest]
#[case::non_release("feature/work", "main")]
#[case::named_base(RELEASE_BRANCH, RELEASE_BRANCH)]
#[tokio::test]
async fn non_release_or_base_branch_is_rejected_before_every_write(
	#[case] branch: &str,
	#[case] base: &str,
	#[values(None, Some(BASE_COMMIT))] head: Option<&str>,
) {
	let server = MockServer::start();
	mock_repository(&server, "main");
	mock_branch(&server, branch, head);
	let writes = mock_commit_writes(&server);
	let create_ref = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/refs");
		then.status(201);
	});
	let update_ref = mock_branch_update(&server, branch, false);

	assert!(
		create_commit(&server, &release_commit_request(branch, Some(base)))
			.await
			.is_err()
	);
	writes.blob.assert_calls(0);
	writes.tree.assert_calls(0);
	writes.commit.assert_calls(0);
	create_ref.assert_calls(0);
	update_ref.assert_calls(0);
}

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
		tree: None,
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
	mock_commit_writes_with(
		server,
		&serde_json::json!({ "sha": RELEASE_COMMIT, "verification": { "verified": true, "reason": "valid" } }),
	)
}

/// The Git Database writes, with GitHub answering the commit with `commit`.
fn mock_commit_writes_with<'a>(
	server: &'a MockServer,
	commit: &serde_json::Value,
) -> CommitWrites<'a> {
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
			then.status(201).json_body(commit.clone());
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
	mock_repository(server, "main");

	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/commits/base-sha");
		then.json_body(serde_json::json!({"tree": {"sha": "base-tree-sha"}}));
	});
	for tree in ["base-tree-sha", "tree-sha"] {
		server.mock(|when, then| {
			when.method(Method::GET)
				.path(format!("/repos/acme/actions/git/trees/{tree}"));
			then.json_body(serde_json::json!({"tree": []}));
		});
	}

	create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"installation-token",
		request,
	)
	.await
}

#[rstest]
#[tokio::test]
async fn first_release_commit_creates_the_release_branch(
	#[values(Some("main"), None)] base: Option<&str>,
) {
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

	let result = create_commit(&server, &release_commit_request(RELEASE_BRANCH, base))
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

#[rstest]
#[case::unsigned(
	serde_json::json!({ "sha": RELEASE_COMMIT, "verification": { "verified": false, "reason": "unsigned" } }),
	"unsigned"
)]
#[case::verification_missing(serde_json::json!({ "sha": RELEASE_COMMIT }), "unknown")]
#[tokio::test]
async fn unverified_release_commit_fails_without_moving_the_branch(
	#[case] commit: serde_json::Value,
	#[case] reason: &str,
) {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some(BASE_COMMIT));
	let writes = mock_commit_writes_with(&server, &commit);
	let update = mock_branch_update(&server, RELEASE_BRANCH, false);

	let error = create_commit(
		&server,
		&release_commit_request(RELEASE_BRANCH, Some("main")),
	)
	.await
	.unwrap_err();

	assert!(
		matches!(
			&error,
			GitHubAppError::UnverifiedCommit(sha, actual) if sha == RELEASE_COMMIT && actual == reason
		),
		"{error}"
	);
	assert!(
		error
			.to_string()
			.contains("the release branch was not moved")
	);
	writes.commit.assert();
	update.assert_calls(0);
}

#[rstest]
#[tokio::test]
async fn release_branch_at_the_base_commit_is_fast_forwarded(
	#[values(Some("main"), None)] base: Option<&str>,
) {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, Some(BASE_COMMIT));
	mock_commit_writes(&server);
	let update = mock_branch_update(&server, RELEASE_BRANCH, false);

	create_commit(&server, &release_commit_request(RELEASE_BRANCH, base))
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

	if base_branch.is_none() {
		assert!(
			matches!(&error, GitHubAppError::BranchMoved(moved, actual, expected)
			if moved == branch && actual == "other-sha" && expected == BASE_COMMIT),
			"{error}"
		);
	} else {
		assert!(
			matches!(&error, GitHubAppError::InvalidReleaseBranch(moved) if moved == branch),
			"{error}"
		);
	}
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
			GitHubAppError::InvalidReleaseBranch(moved) if moved == branch
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

fn release_pull_request() -> monochange_core::SourceChangeRequest {
	serde_json::from_value(serde_json::json!({
		"provider": "github", "repository": "acme/actions", "owner": "acme", "repo": "actions",
		"base_branch": "main", "head_branch": RELEASE_BRANCH,
		"title": "chore(release): prepare release", "body": "Prepared release",
		"labels": ["release"], "auto_merge": true,
		"commit_message": {"subject": "chore(release): prepare release"}
	}))
	.unwrap()
}

fn pull_response() -> serde_json::Value {
	serde_json::json!({
		"number": 42, "node_id": "PR_42", "html_url": "https://github.com/acme/actions/pull/42",
		"body": "An existing PR", "user": {"id": 123, "type": "Bot"},
		"head": {"ref": RELEASE_BRANCH, "repo": {"id": 999}},
		"base": {"ref": "main", "repo": {"id": 999}}
	})
}

const PR_BINDING_SECRET: &str = "test-only-release-pr-signing-key";

fn sign_pull(pull: &mut serde_json::Value, secret: &str) {
	let parsed: super::PullRequestResponse = serde_json::from_value(pull.clone()).unwrap();
	pull["body"] = parsed.signed_body(secret, "Prepared release").into();
}

async fn publish_pull(
	server: &MockServer,
	request: &monochange_core::SourceChangeRequest,
) -> Result<(u64, monochange_core::SourceChangeRequestOperation, String), GitHubAppError> {
	super::publish_release_pull_request(
		&reqwest::Client::new(),
		&server.base_url(),
		"installation-token",
		request,
		PR_BINDING_SECRET,
		999,
	)
	.await
}

#[rstest]
#[case::unmarked("unmarked")]
#[case::wrong_base("base")]
#[case::wrong_head("head")]
#[case::foreign_head_repository("head_repo")]
#[case::foreign_base_repository("base_repo")]
#[case::different_creator("creator")]
#[case::different_number("number")]
#[case::different_node("node")]
#[case::human_creator("human")]
#[case::forged_signature("forged")]
#[case::other_deployment_key("secret")]
#[tokio::test]
async fn unrelated_or_rebound_pull_requests_are_never_mutated(#[case] mismatch: &str) {
	let server = MockServer::start();
	mock_repository(&server, "main");
	let mut pull = pull_response();
	sign_pull(&mut pull, PR_BINDING_SECRET);
	match mismatch {
		"unmarked" => pull["body"] = "An unrelated pull request".into(),
		"base" => pull["base"]["ref"] = "production".into(),
		"head" => pull["head"]["ref"] = "feature/work".into(),
		"head_repo" => pull["head"]["repo"]["id"] = 1000.into(),
		"base_repo" => pull["base"]["repo"]["id"] = 1000.into(),
		"creator" => pull["user"]["id"] = 456.into(),
		"number" => pull["number"] = 43.into(),
		"node" => pull["node_id"] = "PR_43".into(),
		"human" => pull["user"]["type"] = "User".into(),
		"secret" => sign_pull(&mut pull, "another-test-only-deployment-key"),
		"forged" => {
			pull["body"] = format!("{}{} -->", super::RELEASE_PR_MARKER, "00".repeat(32)).into();
		}
		_ => unreachable!(),
	}
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(serde_json::json!([pull]));
	});
	let mutations = server.mock(|when, then| {
		when.method(Method::PATCH);
		then.json_body(pull_response());
	});
	let labels = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/issues/42/labels");
		then.status(200);
	});
	let auto_merge = server.mock(|when, then| {
		when.method(Method::POST).path("/graphql");
		then.json_body(
			serde_json::json!({"data": {"enablePullRequestAutoMerge": {"pullRequest": {"number": 42}}}}),
		);
	});

	assert!(matches!(
		publish_pull(&server, &release_pull_request()).await,
		Err(GitHubAppError::UnboundPullRequest)
	));
	mutations.assert_calls(0);
	labels.assert_calls(0);
	auto_merge.assert_calls(0);
}

#[rstest]
#[case::created(false)]
#[case::bound_update(true)]
#[tokio::test]
async fn only_created_or_authenticated_release_pull_requests_are_updated(#[case] existing: bool) {
	let server = MockServer::start();
	mock_repository(&server, "main");
	let mut pull = pull_response();
	sign_pull(&mut pull, PR_BINDING_SECRET);
	let listed = if existing {
		serde_json::json!([pull.clone()])
	} else {
		serde_json::json!([])
	};
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(listed);
	});
	let create = server.mock(|when, then| {
		when.method(Method::POST).path("/repos/acme/actions/pulls");
		then.status(201).json_body(pull_response());
	});
	let update = server.mock(|when, then| {
		when.method(Method::PATCH)
			.path("/repos/acme/actions/pulls/42")
			.json_body(
				serde_json::json!({"title": "chore(release): prepare release", "body": pull["body"], "base": "main"}),
			);
		then.json_body(pull.clone());
	});
	let labels = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/issues/42/labels");
		then.status(200);
	});
	let auto_merge = server.mock(|when, then| {
		when.method(Method::POST).path("/graphql");
		then.json_body(
			serde_json::json!({"data": {"enablePullRequestAutoMerge": {"pullRequest": {"number": 42}}}}),
		);
	});
	let (_, operation, _) = publish_pull(&server, &release_pull_request())
		.await
		.unwrap();
	assert_eq!(
		operation,
		if existing {
			monochange_core::SourceChangeRequestOperation::Updated
		} else {
			monochange_core::SourceChangeRequestOperation::Created
		}
	);
	create.assert_calls(usize::from(!existing));
	update.assert_calls(1);
	labels.assert_calls(1);
	auto_merge.assert_calls(1);
}

#[rstest]
#[case::truncated_root("truncated")]
#[case::truncated_github("truncated_github")]
#[case::ancestor_blob("ancestor")]
#[case::workflow_blob("workflows")]
#[tokio::test]
async fn workflow_security_tree_boundaries_must_be_complete_directories(
	#[case] invalid: &str,
	#[values(false, true)] uploaded_files: bool,
) {
	let server = MockServer::start();
	mock_branch(&server, RELEASE_BRANCH, None);
	let writes = mock_commit_writes(&server);
	let create_ref = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/refs");
		then.status(201);
	});
	let update_ref = mock_branch_update(&server, RELEASE_BRANCH, false);
	let release_entries = match invalid {
		"truncated" => {
			serde_json::json!({
				"tree": [{"path": ".github", "sha": "github-tree", "type": "tree"}],
				"truncated": true,
			})
		}

		"ancestor" => {
			serde_json::json!({"tree": [{"path": ".github", "sha": "github-tree", "type": "blob"}]})
		}
		_ => {
			serde_json::json!({"tree": [{"path": ".github", "sha": "release-github-tree", "type": "tree"}]})
		}
	};
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/trees/base-tree-sha");
		then.json_body(
			serde_json::json!({"tree": [{"path": ".github", "sha": "github-tree", "type": "tree"}]}),
		);
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/trees/tree-sha");
		then.json_body(release_entries);
	});
	for tree in ["github-tree", "release-github-tree"] {
		let kind = if tree == "release-github-tree" && invalid == "workflows" {
			"blob"
		} else {
			"tree"
		};
		server.mock(|when, then| {
			when.method(Method::GET)
				.path(format!("/repos/acme/actions/git/trees/{tree}"));
			then.json_body(serde_json::json!({
				"tree": [{"path": "workflows", "sha": "workflow-tree", "type": kind}],
				"truncated": tree == "release-github-tree" && invalid == "truncated_github",
			}));
		});
	}
	let mut request = release_commit_request(RELEASE_BRANCH, Some("main"));
	if !uploaded_files {
		request.files.clear();
		request.tree = Some("tree-sha".to_owned());
	}
	let outcome = create_commit(&server, &request).await;
	assert!(
		matches!(&outcome, Err(GitHubAppError::WorkflowsChanged)),
		"{outcome:?}"
	);
	writes.commit.assert_calls(0);
	create_ref.assert_calls(0);
	update_ref.assert_calls(0);
}

#[tokio::test]
async fn ambiguous_release_pull_request_matches_are_not_adopted() {
	let server = MockServer::start();
	mock_repository(&server, "main");
	let mut pull = pull_response();
	sign_pull(&mut pull, PR_BINDING_SECRET);
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(serde_json::json!([pull.clone(), pull]));
	});
	assert!(matches!(
		publish_pull(&server, &release_pull_request()).await,
		Err(GitHubAppError::UnboundPullRequest)
	));
}

#[rstest]
#[case::feature("feature/work", "main")]
#[case::same_as_base(RELEASE_BRANCH, RELEASE_BRANCH)]
#[case::default(RELEASE_BRANCH, "main")]
#[tokio::test]
async fn pull_requests_require_release_branch_policy(#[case] head: &str, #[case] base: &str) {
	let server = MockServer::start();
	mock_repository(&server, RELEASE_BRANCH);
	let mut request = release_pull_request();
	request.head_branch = head.to_owned();
	request.base_branch = base.to_owned();
	assert!(matches!(
		publish_pull(&server, &request).await,
		Err(GitHubAppError::InvalidReleaseBranch(_))
	));
}

#[tokio::test]
async fn a_failed_pull_request_creation_stops_before_all_followup_writes() {
	let server = MockServer::start();
	mock_repository(&server, "main");
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(serde_json::json!([]));
	});
	let failure = r#"{"message":"Validation Failed"}"#;
	let create = server.mock(|when, then| {
		when.method(Method::POST).path("/repos/acme/actions/pulls");
		then.status(422).body(failure);
	});
	let stamp = server.mock(|when, then| {
		when.method(Method::PATCH);
		then.json_body(pull_response());
	});
	let labels = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/issues/42/labels");
		then.status(200);
	});
	let auto_merge = server.mock(|when, then| {
		when.method(Method::POST).path("/graphql");
		then.status(200);
	});

	let outcome = publish_pull(&server, &release_pull_request()).await;
	assert!(
		matches!(&outcome, Err(GitHubAppError::Status("create pull request", status, body))
		if *status == reqwest::StatusCode::UNPROCESSABLE_ENTITY && body.as_str() == failure),
		"{outcome:?}"
	);
	create.assert_calls(1);
	stamp.assert_calls(0);
	labels.assert_calls(0);
	auto_merge.assert_calls(0);
}

#[tokio::test]
async fn a_failed_creation_stamp_has_explicit_recovery_and_cannot_be_adopted_on_retry() {
	let server = MockServer::start();
	mock_repository(&server, "main");
	let mut empty = server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(serde_json::json!([]));
	});
	let create = server.mock(|when, then| {
		when.method(Method::POST).path("/repos/acme/actions/pulls");
		then.status(201).json_body(pull_response());
	});
	let stamp = server.mock(|when, then| {
		when.method(Method::PATCH)
			.path("/repos/acme/actions/pulls/42");
		then.status(503).body("provider unavailable");
	});
	let labels = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/issues/42/labels");
		then.status(200);
	});
	let auto_merge = server.mock(|when, then| {
		when.method(Method::POST).path("/graphql");
		then.status(200);
	});
	let error = publish_pull(&server, &release_pull_request())
		.await
		.unwrap_err();
	assert!(
		matches!(error, GitHubAppError::ReleasePullRequestNeedsRecovery { ref url, ref source }
		if url == "https://github.com/acme/actions/pull/42"
		&& matches!(source.as_ref(), GitHubAppError::Status("update pull request", status, _) if *status == reqwest::StatusCode::SERVICE_UNAVAILABLE))
	);
	empty.delete();
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(serde_json::json!([pull_response()]));
	});
	assert!(matches!(
		publish_pull(&server, &release_pull_request()).await,
		Err(GitHubAppError::UnboundPullRequest)
	));
	create.assert_calls(1);
	stamp.assert_calls(1);
	labels.assert_calls(0);
	auto_merge.assert_calls(0);
}

#[tokio::test]
async fn retargeted_patch_responses_stop_before_labels_or_auto_merge() {
	let server = MockServer::start();
	mock_repository(&server, "main");
	let mut pull = pull_response();
	sign_pull(&mut pull, PR_BINDING_SECRET);
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions/pulls");
		then.json_body(serde_json::json!([pull.clone()]));
	});
	pull["base"]["ref"] = "production".into();
	let update = server.mock(|when, then| {
		when.method(Method::PATCH)
			.path("/repos/acme/actions/pulls/42");
		then.json_body(pull);
	});
	let labels = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/issues/42/labels");
		then.status(200);
	});
	let auto_merge = server.mock(|when, then| {
		when.method(Method::POST).path("/graphql");
		then.status(200);
	});
	assert!(matches!(
		publish_pull(&server, &release_pull_request()).await,
		Err(GitHubAppError::UnboundPullRequest)
	));
	update.assert_calls(1);
	labels.assert_calls(0);
	auto_merge.assert_calls(0);
}
