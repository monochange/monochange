//! GitHub Actions OIDC token verification.
//!
//! The hosted commit endpoint accepts a GitHub Actions OIDC token instead of a
//! long-lived API token. The token proves the request came from a workflow run
//! for a specific repository, which the app maps to a connected installation.
//!
//! Verification:
//! 1. Fetch GitHub's OIDC JWKS from `https://token.actions.githubusercontent.com/.well-known/jwks`.
//! 2. Validate the RS256 signature against the matching JWK.
//! 3. Validate the issuer and the configured audience.
//! 4. Extract the repository claims the endpoint authorizes against.

#[cfg(test)]
#[path = "__tests__/oidc_tests.rs"]
mod tests;

use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;

use jsonwebtoken::DecodingKey;
use jsonwebtoken::Validation;
use jsonwebtoken::decode;
use jsonwebtoken::decode_header;
use jsonwebtoken::jwk::Jwk;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;

/// The only issuer the verifier accepts.
pub const GITHUB_ACTIONS_ISSUER: &str = "https://token.actions.githubusercontent.com";

const JWKS_CACHE_TTL: Duration = Duration::from_secs(600);
const JWKS_RETRY_DELAY: Duration = Duration::from_secs(30);
const JWKS_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const JWKS_MAX_BYTES: usize = 64 * 1024;

/// Errors raised while verifying a GitHub Actions OIDC token.
#[derive(Debug, Error)]
pub enum OidcError {
	#[error("OIDC token is missing or malformed: {0}")]
	InvalidToken(String),
	#[error("failed to fetch GitHub OIDC signing keys: {0}")]
	Jwks(#[from] reqwest::Error),
	#[error("GitHub OIDC signing keys are not valid JSON: {0}")]
	InvalidJwks(#[from] serde_json::Error),
	#[error("GitHub OIDC signing keys exceed the response size limit")]
	JwksTooLarge,
	#[error("GitHub OIDC signing key refresh is temporarily unavailable")]
	RefreshBackoff,
	#[error("no signing key matches the token header")]
	NoMatchingKey,
	#[error("OIDC token validation failed: {0}")]
	Validation(String),
	#[error("repository `{0}` is not connected to monochange")]
	UnknownRepository(String),
}

/// The GitHub Actions OIDC claims the endpoint authorizes against.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OidcClaims {
	/// Repository full name, e.g. `owner/repo`.
	#[serde(rename = "repository")]
	pub repository: String,
	/// Numeric repository id.
	#[serde(rename = "repository_id")]
	pub repository_id: String,
	/// Repository owner login.
	#[serde(rename = "repository_owner")]
	pub repository_owner: String,
	/// Ref the workflow ran on, e.g. `refs/heads/main`.
	#[serde(default)]
	pub r#ref: String,
	/// Commit SHA the workflow checked out.
	#[serde(default)]
	pub sha: String,
	/// Workflow name.
	#[serde(default)]
	pub workflow: String,
	/// `owner/repo/path@ref` of the workflow definition.
	#[serde(default, rename = "job_workflow_ref")]
	pub job_workflow_ref: String,
	/// Workflow run id.
	#[serde(default, rename = "run_id")]
	pub run_id: String,
	/// Workflow run attempt.
	#[serde(default, rename = "run_attempt")]
	pub run_attempt: String,
	/// Actor that triggered the run.
	#[serde(default)]
	pub actor: String,
	/// Event name, e.g. `push`.
	#[serde(default)]
	pub event_name: String,
}

/// A cached JWKS with its fetch time, refreshed well before keys rotate.
struct CachedJwks {
	keys_by_kid: HashMap<String, Jwk>,
	fetched_at: Instant,
}

#[derive(Default)]
struct JwksCache {
	document: Option<CachedJwks>,
	retry_after: Option<Instant>,
}

impl JwksCache {
	/// A fresh document answers even unknown key IDs without a refresh.
	fn fresh_key(&self, kid: &str) -> Option<Result<Jwk, OidcError>> {
		self.document
			.as_ref()
			.filter(|document| document.fetched_at.elapsed() < JWKS_CACHE_TTL)
			.map(|document| {
				document
					.keys_by_kid
					.get(kid)
					.cloned()
					.ok_or(OidcError::NoMatchingKey)
			})
	}
}

/// Verifies GitHub Actions OIDC tokens against GitHub's published JWKS.
pub struct OidcVerifier {
	issuer: String,
	audience: String,
	jwks_url: String,
	http: reqwest::Client,
	cache: Mutex<JwksCache>,
	refresh: Semaphore,
}

impl OidcVerifier {
	/// Build a verifier for a monochange deployment audience.
	#[must_use]
	pub fn new(audience: &str) -> Self {
		Self {
			issuer: GITHUB_ACTIONS_ISSUER.to_string(),
			audience: audience.to_string(),
			jwks_url: format!("{GITHUB_ACTIONS_ISSUER}/.well-known/jwks"),
			http: reqwest::Client::builder()
				.connect_timeout(Duration::from_secs(10))
				.timeout(JWKS_REQUEST_TIMEOUT)
				.build()
				.unwrap_or_default(),
			cache: Mutex::new(JwksCache::default()),
			refresh: Semaphore::new(1),
		}
	}

	/// Override the JWKS URL; tests point this at a local server.
	#[must_use]
	pub fn with_jwks_url(mut self, url: &str) -> Self {
		self.jwks_url = url.to_string();
		self
	}

	/// Verify an OIDC token and return its repository claims.
	pub async fn verify(&self, token: &str) -> Result<OidcClaims, OidcError> {
		let header =
			decode_header(token).map_err(|error| OidcError::InvalidToken(error.to_string()))?;
		let Some(kid) = header.kid else {
			return Err(OidcError::InvalidToken(
				"token header has no key id".to_string(),
			));
		};
		let jwk = self.jwk_for_kid(&kid).await?;
		let key = DecodingKey::from_jwk(&jwk)
			.map_err(|error| OidcError::InvalidToken(error.to_string()))?;
		let mut validation = Validation::new(header.alg);
		validation.set_issuer(std::slice::from_ref(&self.issuer));

		validation.set_audience(std::slice::from_ref(&self.audience));
		validation.validate_exp = true;
		let data = decode::<OidcClaims>(token, &key, &validation).map_err(|error| {
			OidcError::Validation(format!(
				"token rejected (issuer `{}`, audience `{}`): {error}",
				self.issuer, self.audience
			))
		})?;

		if data.claims.repository.is_empty() {
			return Err(OidcError::InvalidToken(
				"token carries no repository claim".to_string(),
			));
		}

		Ok(data.claims)
	}

	async fn jwk_for_kid(&self, kid: &str) -> Result<Jwk, OidcError> {
		let cached = self.cache.lock().await.fresh_key(kid);

		if let Some(key) = cached {
			return key;
		}

		// One request owns the refresh; no cache data lock spans network work.
		let _permit = self
			.refresh
			.acquire()
			.await
			.unwrap_or_else(|_| panic!("the private JWKS refresh gate is never closed"));
		{
			let mut cache = self.cache.lock().await;

			if let Some(key) = cache.fresh_key(kid) {
				return key;
			}

			if cache
				.retry_after
				.is_some_and(|deadline| Instant::now() < deadline)
			{
				return Err(OidcError::RefreshBackoff);
			}

			// Cancellation must also leave a deadline for the next owner.
			cache.retry_after = Some(Instant::now() + JWKS_RETRY_DELAY);
		}

		let fetched = self.fetch_signing_keys().await;
		let mut cache = self.cache.lock().await;
		cache.retry_after = Some(Instant::now() + JWKS_RETRY_DELAY);
		let keys_by_kid = fetched?;
		let key = keys_by_kid.get(kid).cloned();
		cache.document = Some(CachedJwks {
			keys_by_kid,
			fetched_at: Instant::now(),
		});
		cache.retry_after = None;

		key.ok_or(OidcError::NoMatchingKey)
	}

	async fn fetch_signing_keys(&self) -> Result<HashMap<String, Jwk>, OidcError> {
		let mut response = self
			.http
			.get(&self.jwks_url)
			.timeout(JWKS_REQUEST_TIMEOUT)
			.send()
			.await?
			.error_for_status()?;
		let mut bytes = Vec::new();

		while let Some(chunk) = response.chunk().await? {
			if chunk.len() > JWKS_MAX_BYTES - bytes.len() {
				return Err(OidcError::JwksTooLarge);
			}

			bytes.extend_from_slice(&chunk);
		}

		let document: JwksDocument = serde_json::from_slice(&bytes)?;

		Ok(document
			.keys
			.into_iter()
			.filter_map(|jwk| jwk.common.key_id.clone().map(|kid| (kid, jwk)))
			.collect())
	}
}

#[derive(Debug, Deserialize)]
struct JwksDocument {
	keys: Vec<Jwk>,
}
