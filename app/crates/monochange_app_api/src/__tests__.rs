//! Unit tests for the API layer (JWT, claims, state).

// `#[tokio::test]` generates `Builder::expect("Failed building the Runtime")`
// internally; the app code below never calls `Result::expect` directly.
#![allow(clippy::disallowed_methods)]

use rstest::rstest;

use crate::*;

// ── JWT token tests ──

#[rstest]
fn test_create_and_verify_token() {
	let secret = "test-secret-key-123";
	let user_id = 42;
	let github_id = 123_456;
	let login = "testuser";

	let token = create_token(secret, user_id, github_id, login)
		.unwrap_or_else(|error| panic!("Token creation should succeed: {error}"));
	assert!(!token.is_empty());

	let claims = verify_token(secret, &token)
		.unwrap_or_else(|error| panic!("Token verification should succeed: {error}"));
	assert_eq!(claims.sub, user_id);
	assert_eq!(claims.github_id, github_id);
	assert_eq!(claims.github_login, login);
}

#[rstest]
fn test_verify_invalid_token() {
	let result = verify_token("secret", "invalid.token.here");
	assert!(result.is_err());
}

#[rstest]
fn test_verify_wrong_secret() {
	let secret = "correct-secret";
	let token = create_token(secret, 1, 1, "user").unwrap();
	let result = verify_token("wrong-secret", &token);
	assert!(result.is_err());
}

#[rstest]
fn test_verify_empty_token() {
	let result = verify_token("secret", "");
	assert!(result.is_err());
}

#[rstest]
fn test_verify_malformed_token() {
	let result = verify_token("secret", "not.a.jwt");
	assert!(result.is_err());
}

#[rstest]
fn test_token_is_unique() {
	let secret = "secret";
	let token1 = create_token(secret, 1, 100, "a").unwrap();
	let token2 = create_token(secret, 2, 200, "b").unwrap();
	assert_ne!(token1, token2);
}

#[rstest]
fn test_token_contains_expected_claims() {
	let secret = "my-secret";
	let token = create_token(secret, 7, 999, "example").unwrap();
	let claims = verify_token(secret, &token).unwrap();
	assert_eq!(claims.sub, 7);
	assert_eq!(claims.github_id, 999);
	assert_eq!(claims.github_login, "example");
}

#[rstest]
fn test_token_expiry_is_future() {
	let secret = "secret";
	let token = create_token(secret, 1, 1, "user").unwrap();
	let claims = verify_token(secret, &token).unwrap();
	let now = chrono::Utc::now().timestamp() as usize;
	assert!(claims.exp > now, "Token expiry should be in the future");
}

#[rstest]
fn test_token_with_special_characters_in_login() {
	let secret = "secret";
	let login = "user-name_test.123";
	let token = create_token(secret, 1, 1, login).unwrap();
	let claims = verify_token(secret, &token).unwrap();
	assert_eq!(claims.github_login, login);
}

// ── Claims serialization tests ──

#[rstest]
fn test_claims_serialization_roundtrip() {
	let claims = Claims {
		sub: 10,
		github_id: 555,
		github_login: "serializer".to_string(),
		exp: 9_999_999_999,
		iat: 1_111_111_111,
	};

	let json = serde_json::to_string(&claims).unwrap_or_else(|error| panic!("Serialize: {error}"));
	let parsed: Claims =
		serde_json::from_str(&json).unwrap_or_else(|error| panic!("Deserialize: {error}"));

	assert_eq!(parsed.sub, claims.sub);
	assert_eq!(parsed.github_id, claims.github_id);
	assert_eq!(parsed.github_login, claims.github_login);
}

#[rstest]
fn test_claims_json_has_expected_fields() {
	let claims = Claims {
		sub: 1,
		github_id: 2,
		github_login: "test".into(),
		exp: 100,
		iat: 50,
	};
	let json = serde_json::to_string(&claims).unwrap();
	assert!(json.contains("\"sub\":1"));
	assert!(json.contains("\"github_id\":2"));
	assert!(json.contains("\"github_login\":\"test\""));
}

// ── GitHub App webhook signature tests ──

mod webhook_tests {
	use hmac::Hmac;
	use hmac::Mac;
	use sha2::Sha256;

	use crate::github_app::GitHubAppAuth;

	fn test_auth() -> GitHubAppAuth {
		GitHubAppAuth::new("123", "", "webhook-secret", "https://api.github.com")
	}

	fn sign(secret: &str, payload: &[u8]) -> String {
		let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
			.unwrap_or_else(|error| panic!("key: {error}"));
		mac.update(payload);
		format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
	}

	#[test]
	fn accepts_valid_signature() {
		let auth = test_auth();
		let payload = br#"{"action":"created"}"#;
		let signature = sign("webhook-secret", payload);
		assert!(auth.verify_webhook_signature(payload, &signature).is_ok());
	}

	#[test]
	fn rejects_tampered_payload() {
		let auth = test_auth();
		let payload = br#"{"action":"created"}"#;
		let signature = sign("webhook-secret", payload);
		assert!(
			auth.verify_webhook_signature(b"tampered", &signature)
				.is_err()
		);
	}

	#[test]
	fn rejects_wrong_secret() {
		let auth = test_auth();
		let payload = br#"{"action":"created"}"#;
		let signature = sign("other-secret", payload);
		assert!(auth.verify_webhook_signature(payload, &signature).is_err());
	}

	#[test]
	fn rejects_missing_signature() {
		let auth = test_auth();
		assert!(auth.verify_webhook_signature(b"payload", "").is_err());
	}

	#[test]
	fn unconfigured_webhook_reports_not_configured() {
		let auth = GitHubAppAuth::new("123", "", "", "https://api.github.com");
		assert!(!auth.webhook_configured());
	}
}

// ── GitHub Actions OIDC verification tests ──

mod oidc_tests {
	use base64::Engine;
	use base64::engine::general_purpose::URL_SAFE_NO_PAD;
	use httpmock::Method;
	use httpmock::MockServer;
	use jsonwebtoken::Algorithm;
	use jsonwebtoken::EncodingKey;
	use jsonwebtoken::Header;
	use jsonwebtoken::encode;
	use rsa::pkcs1::EncodeRsaPrivateKey;
	use rsa::traits::PublicKeyParts;

	use crate::oidc::OidcVerifier;

	/// A throwaway RSA keypair generated at test time, so no key material is
	/// ever committed to the repository.
	struct TestKeys {
		pkcs1_der: Vec<u8>,
		jwks_json: String,
	}

	fn test_keys() -> TestKeys {
		use rsa::RsaPrivateKey;
		use rsa::rand_core::OsRng;

		let private = RsaPrivateKey::new(&mut OsRng, 2048)
			.unwrap_or_else(|error| panic!("generate test rsa key: {error}"));
		let pkcs1_der = private
			.to_pkcs1_der()
			.unwrap_or_else(|error| panic!("encode test key: {error}"))
			.as_bytes()
			.to_vec();
		let n = URL_SAFE_NO_PAD.encode(private.n().to_bytes_be());
		let e = URL_SAFE_NO_PAD.encode(private.e().to_bytes_be());
		let jwks_json = serde_json::json!({
			"keys": [
				{
					"kty": "RSA",
					"kid": "test-key-1",
					"use": "sig",
					"alg": "RS256",
					"n": n,
					"e": e,
				}
			]
		})
		.to_string();
		TestKeys {
			pkcs1_der,
			jwks_json,
		}
	}

	fn signed_token(keys: &TestKeys, issuer: &str, audience: &str, repository: &str) -> String {
		let now = chrono::Utc::now().timestamp();
		let claims = serde_json::json!({
			"iss": issuer,
			"aud": audience,
			"exp": now + 600,
			"iat": now,
			"repository": repository,
			"repository_id": "42",
			"repository_owner": "monochange",
			"ref": "refs/heads/main",
			"sha": "abc123",
			"workflow": "release",
			"job_workflow_ref": "monochange/monochange/.github/workflows/release.yml@refs/heads/main",
			"run_id": "100",
			"run_attempt": "1",
			"actor": "ifiokjr",
			"event_name": "push",
		});
		encode(
			&Header {
				alg: Algorithm::RS256,
				kid: Some("test-key-1".to_string()),
				..Default::default()
			},
			&claims,
			&EncodingKey::from_rsa_der(&keys.pkcs1_der),
		)
		.unwrap_or_else(|error| panic!("sign test token: {error}"))
	}

	fn verifier_with(server: &MockServer, keys: &TestKeys) -> OidcVerifier {
		server.mock(|when, then| {
			when.method(Method::GET).path("/.well-known/jwks");
			then.status(200).body(keys.jwks_json.clone());
		});
		OidcVerifier::new("monochange.dev").with_jwks_url(&server.url("/.well-known/jwks"))
	}

	#[tokio::test]
	async fn verifies_valid_token() {
		let keys = test_keys();
		let server = MockServer::start();
		let jwks = server.mock(|when, then| {
			when.method(Method::GET).path("/.well-known/jwks");
			then.status(200).body(keys.jwks_json.clone());
		});
		let verifier = verifier_with(&server, &keys);

		let token = signed_token(
			&keys,
			"https://token.actions.githubusercontent.com",
			"monochange.dev",
			"monochange/monochange",
		);
		let claims = verifier
			.verify(&token)
			.await
			.unwrap_or_else(|error| panic!("valid token should verify: {error}"));
		assert_eq!(claims.repository, "monochange/monochange");
		assert_eq!(claims.repository_id, "42");
		assert_eq!(claims.repository_owner, "monochange");
		jwks.assert();
	}

	#[tokio::test]
	async fn rejects_wrong_audience() {
		let keys = test_keys();
		let server = MockServer::start();
		let verifier = verifier_with(&server, &keys);

		let token = signed_token(
			&keys,
			"https://token.actions.githubusercontent.com",
			"other-service",
			"monochange/monochange",
		);
		assert!(verifier.verify(&token).await.is_err());
	}

	#[tokio::test]
	async fn rejects_wrong_issuer() {
		let keys = test_keys();
		let server = MockServer::start();
		let verifier = verifier_with(&server, &keys);

		let token = signed_token(
			&keys,
			"https://evil.example.com",
			"monochange.dev",
			"monochange/monochange",
		);
		assert!(verifier.verify(&token).await.is_err());
	}

	#[tokio::test]
	async fn rejects_garbage_token() {
		let verifier = OidcVerifier::new("monochange.dev");
		assert!(verifier.verify("not-a-token").await.is_err());
		assert!(verifier.verify("").await.is_err());
	}
}
#[path = "__tests__/installation_tests.rs"]
mod installation_tests;
