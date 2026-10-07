//! Authenticated app identity determines the repository connection link.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::OnceLock;

use httpmock::Method;
use httpmock::MockServer;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;
use rstest::rstest;

use super::GitHubAppAuth;
use super::GitHubAppError;

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
