//! Resolved GitHub App credential validation.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use hmac::Hmac;
use hmac::Mac;
use httpmock::Method;
use httpmock::MockServer;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;
use sha2::Sha256;

use crate::AppSecrets;
use crate::config::github_app_credentials;
use crate::github_app::GitHubAppError;

/// Resolved secrets with every required field set and the given optional
/// GitHub App fields layered on top.
fn secrets(optional: serde_json::Value) -> AppSecrets {
	let mut values = serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "config-test-signing-key",
		"github_client_id": "",
		"github_client_secret": "",
	});
	let optional: serde_json::Map<String, serde_json::Value> =
		serde_json::from_value(optional).unwrap();
	values.as_object_mut().unwrap().extend(optional);
	serde_json::from_value(values).unwrap()
}

#[test]
fn no_bot_credentials_allows_local_development() {
	assert!(
		github_app_credentials(&secrets(serde_json::json!({})))
			.unwrap()
			.is_none()
	);
	assert!(
		github_app_credentials(&secrets(serde_json::json!({
			"github_app_id": " ", "github_app_private_key": "", "github_app_webhook_secret": "",
		})))
		.unwrap()
		.is_none()
	);
}

#[test]
fn incomplete_bot_credentials_fail_instead_of_silently_disabling_the_bot() {
	for values in [
		serde_json::json!({"github_app_id": "123"}),
		serde_json::json!({"github_app_private_key": "a-key"}),
		serde_json::json!({"github_app_webhook_secret": "a-secret"}),
		serde_json::json!({"github_app_id": "123", "github_app_private_key": "a-key"}),
		serde_json::json!({"github_app_id": "123", "github_app_webhook_secret": "a-secret"}),
		serde_json::json!({"github_app_private_key": "a-key", "github_app_webhook_secret": "a-secret"}),
	] {
		assert!(matches!(
			github_app_credentials(&secrets(values)),
			Err(GitHubAppError::NotConfigured)
		));
	}
}

#[test]
fn invalid_pem_is_rejected_during_startup() {
	let values = secrets(serde_json::json!({
		"github_app_id": "123", "github_app_private_key": "not-a-pem", "github_app_webhook_secret": "a-secret",
	}));
	assert!(matches!(
		github_app_credentials(&values),
		Err(GitHubAppError::Jwt(_))
	));
}

#[tokio::test]
async fn uses_resolved_pem_and_webhook_secret_without_environment_export() {
	let key = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
	let pem = key.to_pkcs1_pem(LineEnding::LF).unwrap();
	let values = secrets(serde_json::json!({
		"github_app_id": "123", "github_app_private_key": pem.as_str(), "github_app_webhook_secret": "a-secret",
	}));
	let mut auth = github_app_credentials(&values).unwrap().unwrap();
	assert_eq!(auth.app_id, "123");
	assert!(auth.webhook_configured());
	let payload = b"installation-test";
	let mut signature = Hmac::<Sha256>::new_from_slice(b"a-secret").unwrap();
	signature.update(payload);
	let signature = format!("sha256={}", hex::encode(signature.finalize().into_bytes()));
	assert!(auth.verify_webhook_signature(payload, &signature).is_ok());
	let server = MockServer::start();
	let request = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/app/installations/456/access_tokens")
			.header_exists("authorization");
		then.status(201)
			.json_body(serde_json::json!({"token": "installation-test-token"}));
	});
	auth.api_url = server.base_url();
	assert_eq!(
		auth.installation_token(&reqwest::Client::new(), 456)
			.await
			.unwrap(),
		"installation-test-token",
	);
	request.assert();
}
