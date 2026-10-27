//! Untrusted key identifiers must never determine the JWKS fetch rate.

#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use httpmock::Method::GET;
use httpmock::MockServer;
use rstest::rstest;

use super::JWKS_CACHE_TTL;
use super::JWKS_MAX_BYTES;
use super::JWKS_REQUEST_TIMEOUT;
use super::OidcError;
use super::OidcVerifier;

fn jwks() -> serde_json::Value {
	serde_json::json!({
		"keys": [{
			"kty": "RSA",
			"kid": "known-key",
			"use": "sig",
			"alg": "RS256",
			"n": "AQ",
			"e": "AQAB",
		}],
	})
}

fn token_with_kid(kid: &str) -> String {
	let header = serde_json::json!({"alg": "RS256", "kid": kid});
	format!("{}.e30.AA", URL_SAFE_NO_PAD.encode(header.to_string()))
}

fn verifier(server: &MockServer) -> OidcVerifier {
	OidcVerifier::new("monochange.dev").with_jwks_url(&server.url("/jwks"))
}

#[tokio::test]
async fn fresh_cache_rejects_distinct_unknown_key_ids_without_provider_calls() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.json_body(jwks());
		})
		.await;
	let verifier = verifier(&server);
	verifier.jwk_for_kid("known-key").await.unwrap();

	for index in 0..32 {
		assert!(matches!(
			verifier
				.verify(&token_with_kid(&format!("unknown-{index}")))
				.await,
			Err(OidcError::NoMatchingKey),
		));
	}

	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn concurrent_cold_cache_requests_share_one_provider_call() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.delay(Duration::from_millis(50)).json_body(jwks());
		})
		.await;
	let verifier = Arc::new(verifier(&server));
	let mut requests = tokio::task::JoinSet::new();

	for _ in 0..16 {
		let verifier = Arc::clone(&verifier);
		requests.spawn(async move { verifier.jwk_for_kid("known-key").await });
	}

	while let Some(result) = requests.join_next().await {
		assert!(result.unwrap().is_ok());
	}

	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn concurrent_unknown_key_ids_share_one_provider_call() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.delay(Duration::from_millis(50))
				.json_body(serde_json::json!({"keys": []}));
		})
		.await;
	let verifier = Arc::new(verifier(&server));
	let mut requests = tokio::task::JoinSet::new();

	for index in 0..16 {
		let verifier = Arc::clone(&verifier);
		requests.spawn(async move { verifier.jwk_for_kid(&format!("unknown-{index}")).await });
	}

	while let Some(result) = requests.join_next().await {
		assert!(matches!(result.unwrap(), Err(OidcError::NoMatchingKey)));
	}

	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn failed_refresh_cannot_be_retried_for_each_untrusted_key_id() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.status(503).body("provider unavailable");
		})
		.await;
	let verifier = verifier(&server);

	for index in 0..16 {
		assert!(
			verifier
				.jwk_for_kid(&format!("unknown-{index}"))
				.await
				.is_err()
		);
	}

	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn concurrent_failed_requests_share_one_provider_call() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.delay(Duration::from_millis(50)).status(503);
		})
		.await;
	let verifier = Arc::new(verifier(&server));
	let mut requests = tokio::task::JoinSet::new();

	for index in 0..16 {
		let verifier = Arc::clone(&verifier);
		requests.spawn(async move { verifier.jwk_for_kid(&format!("unknown-{index}")).await });
	}

	while let Some(result) = requests.join_next().await {
		assert!(result.unwrap().is_err());
	}

	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn concurrent_expired_cache_requests_share_one_refresh() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.delay(Duration::from_millis(50)).json_body(jwks());
		})
		.await;
	let verifier = Arc::new(verifier(&server));
	verifier.jwk_for_kid("known-key").await.unwrap();
	verifier
		.cache
		.lock()
		.await
		.document
		.as_mut()
		.unwrap()
		.fetched_at = Instant::now() - JWKS_CACHE_TTL;
	let mut requests = tokio::task::JoinSet::new();

	for index in 0..16 {
		let verifier = Arc::clone(&verifier);
		requests.spawn(async move { verifier.jwk_for_kid(&format!("unknown-{index}")).await });
	}

	while let Some(result) = requests.join_next().await {
		assert!(matches!(result.unwrap(), Err(OidcError::NoMatchingKey)));
	}

	provider.assert_calls_async(2).await;
}

#[tokio::test]
async fn provider_error_preserves_last_good_keys_without_accepting_expired_keys() {
	let server = MockServer::start_async().await;
	let healthy = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.json_body(jwks());
		})
		.await;
	let verifier = verifier(&server);
	verifier.jwk_for_kid("known-key").await.unwrap();
	healthy.delete_async().await;
	let failed = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.status(503).json_body(jwks());
		})
		.await;
	let expired_at = Instant::now() - JWKS_CACHE_TTL;
	verifier
		.cache
		.lock()
		.await
		.document
		.as_mut()
		.unwrap()
		.fetched_at = expired_at;

	assert!(matches!(
		verifier.jwk_for_kid("known-key").await,
		Err(OidcError::Jwks(error)) if error.status() == Some(reqwest::StatusCode::SERVICE_UNAVAILABLE),
	));
	assert!(matches!(
		verifier.jwk_for_kid("known-key").await,
		Err(OidcError::RefreshBackoff)
	));
	let cache = verifier.cache.lock().await;
	let document = cache.document.as_ref().unwrap();
	assert!(document.keys_by_kid.contains_key("known-key"));
	assert_eq!(document.fetched_at, expired_at);
	drop(cache);
	failed.assert_calls_async(1).await;
}

#[tokio::test]
async fn refresh_recovers_after_the_failure_deadline() {
	let server = MockServer::start_async().await;
	let failed = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.status(503);
		})
		.await;
	let verifier = verifier(&server);
	assert!(verifier.jwk_for_kid("known-key").await.is_err());
	failed.delete_async().await;
	let healthy = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.json_body(jwks());
		})
		.await;
	verifier.cache.lock().await.retry_after = Some(Instant::now() - Duration::from_secs(1));

	verifier.jwk_for_kid("known-key").await.unwrap();
	verifier.jwk_for_kid("known-key").await.unwrap();
	healthy.assert_calls_async(1).await;
}

#[rstest]
#[case(JWKS_MAX_BYTES, true)]
#[case(JWKS_MAX_BYTES + 1, false)]
#[tokio::test]
async fn response_size_is_bounded_before_json_parsing(#[case] size: usize, #[case] accepted: bool) {
	let server = MockServer::start_async().await;
	let mut body = jwks().to_string();
	body.push_str(&" ".repeat(size - body.len()));
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.body(body);
		})
		.await;
	let verifier = verifier(&server);
	let result = verifier.jwk_for_kid("known-key").await;

	if accepted {
		assert!(result.is_ok());
	} else {
		assert!(matches!(result, Err(OidcError::JwksTooLarge)));
		assert!(matches!(
			verifier.jwk_for_kid("known-key").await,
			Err(OidcError::RefreshBackoff)
		));
	}

	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn invalid_json_uses_the_failure_deadline() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.body("not JSON");
		})
		.await;
	let verifier = verifier(&server);

	assert!(matches!(
		verifier.jwk_for_kid("known-key").await,
		Err(OidcError::InvalidJwks(_))
	));
	assert!(matches!(
		verifier.jwk_for_kid("unknown-key").await,
		Err(OidcError::RefreshBackoff)
	));
	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn timed_out_refresh_uses_the_failure_deadline() {
	let server = MockServer::start_async().await;
	let provider = server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.delay(JWKS_REQUEST_TIMEOUT * 2).json_body(jwks());
		})
		.await;
	let verifier = verifier(&server);

	assert!(matches!(
		verifier.jwk_for_kid("known-key").await,
		Err(OidcError::Jwks(error)) if error.is_timeout(),
	));
	assert!(matches!(
		verifier.jwk_for_kid("unknown-key").await,
		Err(OidcError::RefreshBackoff)
	));
	provider.assert_calls_async(1).await;
}

#[tokio::test]
async fn cancelled_refresh_keeps_the_failure_deadline() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/jwks");
			then.delay(Duration::from_secs(2)).json_body(jwks());
		})
		.await;
	let verifier = verifier(&server);

	assert!(
		tokio::time::timeout(
			Duration::from_millis(100),
			verifier.jwk_for_kid("known-key")
		)
		.await
		.is_err()
	);
	assert!(matches!(
		verifier.jwk_for_kid("unknown-key").await,
		Err(OidcError::RefreshBackoff)
	));
}
