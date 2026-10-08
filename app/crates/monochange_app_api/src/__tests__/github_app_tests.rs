//! Tests for the monochange GitHub App identity and release-commit creation.
//!
//! The authenticated app identity determines the repository connection link,
//! and the hosted release-commit contract — including the hard failure when
//! GitHub does not verify a commit — stays pinned to observable behavior.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::OnceLock;

use httpmock::Method;
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

// ── Hosted release-commit creation tests ──

const OWNER: &str = "monochange";
const REPO: &str = "monochange";
const BRANCH: &str = "monochange/release/main";
const BASE: &str = "1111111111111111111111111111111111111111";
const BLOB: &str = "2222222222222222222222222222222222222222";
const TREE: &str = "3333333333333333333333333333333333333333";
const COMMIT: &str = "4444444444444444444444444444444444444444";

fn request() -> HostedCommitRequest {
	HostedCommitRequest {
		provider: "github".to_string(),
		owner: OWNER.to_string(),
		repository: REPO.to_string(),
		branch: BRANCH.to_string(),
		base_commit: BASE.to_string(),
		subject: "chore(release): prepare release".to_string(),
		body: "body".to_string(),
		files: vec![
			HostedCommitFile {
				path: "crates/monochange/Cargo.toml".to_string(),
				content: Some("version = \"1.2.3\"".to_string()),
			},
			HostedCommitFile {
				path: ".changeset/consumed.md".to_string(),
				content: None,
			},
		],
		dry_run: false,
		idempotency_key: None,
	}
}

fn mock_blob(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(Method::POST)
			.path(format!("/repos/{OWNER}/{REPO}/git/blobs"));
		then.status(201)
			.json_body(serde_json::json!({ "sha": BLOB }));
	})
}

fn mock_ref_at_base(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(Method::GET)
			.path(format!("/repos/{OWNER}/{REPO}/git/ref/heads/{BRANCH}"));
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": BASE } }));
	})
}

fn mock_tree(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(Method::POST)
			.path(format!("/repos/{OWNER}/{REPO}/git/trees"));
		then.status(201)
			.json_body(serde_json::json!({ "sha": TREE }));
	})
}

fn mock_commit(server: &MockServer, verification: serde_json::Value) -> httpmock::Mock<'_> {
	server.mock(move |when, then| {
		when.method(Method::POST)
			.path(format!("/repos/{OWNER}/{REPO}/git/commits"));
		then.status(201).json_body(serde_json::json!({
			"sha": COMMIT,
			"verification": verification,
		}));
	})
}

fn mock_update_ref(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(Method::PATCH)
			.path(format!("/repos/{OWNER}/{REPO}/git/refs/heads/{BRANCH}"));
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": COMMIT } }));
	})
}

#[tokio::test]
async fn creates_branch_and_reports_verified_commit() {
	let server = MockServer::start_async().await;
	mock_blob(&server);
	mock_tree(&server);
	mock_commit(
		&server,
		serde_json::json!({ "verified": true, "reason": "valid" }),
	);
	let head_missing = server.mock(|when, then| {
		when.method(Method::GET)
			.path(format!("/repos/{OWNER}/{REPO}/git/ref/heads/{BRANCH}"));
		then.status(404);
	});
	let create_ref = server.mock(|when, then| {
		when.method(Method::POST)
			.path(format!("/repos/{OWNER}/{REPO}/git/refs"));
		then.status(201);
	});
	let update_ref = server.mock(|when, then| {
		when.method(Method::PATCH)
			.path(format!("/repos/{OWNER}/{REPO}/git/refs/heads/{BRANCH}"));
		then.status(200);
	});

	let (sha, verified, reason) = create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"token",
		&request(),
	)
	.await
	.unwrap_or_else(|error| panic!("hosted commit should succeed: {error}"));

	assert_eq!(sha, COMMIT);
	assert!(verified);
	assert_eq!(reason.as_deref(), Some("valid"));
	head_missing.assert_async().await;
	create_ref.assert_async().await;
	update_ref.assert_calls_async(0).await;
}

#[tokio::test]
async fn moves_existing_branch_and_reports_verified_commit() {
	let server = MockServer::start_async().await;
	mock_blob(&server);
	mock_ref_at_base(&server);
	mock_tree(&server);
	mock_commit(
		&server,
		serde_json::json!({ "verified": true, "reason": "valid" }),
	);
	let update_ref = mock_update_ref(&server);

	let (sha, verified, _) = create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"token",
		&request(),
	)
	.await
	.unwrap_or_else(|error| panic!("hosted commit should succeed: {error}"));

	assert_eq!(sha, COMMIT);
	assert!(verified);
	update_ref.assert_async().await;
}

#[tokio::test]
async fn fails_without_moving_branch_when_github_does_not_verify() {
	let server = MockServer::start_async().await;
	mock_blob(&server);
	mock_ref_at_base(&server);
	mock_tree(&server);
	mock_commit(
		&server,
		serde_json::json!({ "verified": false, "reason": "unsigned" }),
	);
	let update_ref = server.mock(|when, then| {
		when.method(Method::PATCH)
			.path(format!("/repos/{OWNER}/{REPO}/git/refs/heads/{BRANCH}"));
		then.status(200);
	});

	let error = create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"token",
		&request(),
	)
	.await
	.expect_err("an unverified commit must fail");

	assert!(matches!(error, GitHubAppError::UnverifiedCommit(..)));
	let message = error.to_string();
	assert!(
		message.contains(COMMIT),
		"message should name the commit: {message}"
	);
	assert!(
		message.contains("unsigned"),
		"message should include the reason: {message}"
	);
	update_ref.assert_calls_async(0).await;
}

#[tokio::test]
async fn fails_when_verification_is_absent_from_commit_response() {
	let server = MockServer::start_async().await;
	mock_blob(&server);
	mock_ref_at_base(&server);
	mock_tree(&server);
	let no_verification = server.mock(|when, then| {
		when.method(Method::POST)
			.path(format!("/repos/{OWNER}/{REPO}/git/commits"));
		then.status(201)
			.json_body(serde_json::json!({ "sha": COMMIT }));
	});

	let error = create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"token",
		&request(),
	)
	.await
	.expect_err("a commit with no verification object must fail");

	assert!(matches!(error, GitHubAppError::UnverifiedCommit(..)));
	assert!(error.to_string().contains("unknown"));
	no_verification.assert_async().await;
}

#[tokio::test]
async fn rejects_a_branch_that_moved_since_preparation() {
	let server = MockServer::start_async().await;
	mock_blob(&server);
	server.mock(|when, then| {
		when.method(Method::GET)
			.path(format!("/repos/{OWNER}/{REPO}/git/ref/heads/{BRANCH}"));
		then.status(200).json_body(
			serde_json::json!({ "object": { "sha": "9999999999999999999999999999999999999999" } }),
		);
	});
	let tree = server.mock(|when, then| {
		when.method(Method::POST)
			.path(format!("/repos/{OWNER}/{REPO}/git/trees"));
		then.status(201)
			.json_body(serde_json::json!({ "sha": TREE }));
	});

	let error = create_release_commit(
		&reqwest::Client::new(),
		&server.base_url(),
		"token",
		&request(),
	)
	.await
	.expect_err("a moved branch must fail before any write");

	assert!(matches!(error, GitHubAppError::BranchMoved(..)));
	tree.assert_calls_async(0).await;
}
