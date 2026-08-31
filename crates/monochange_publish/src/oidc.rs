//! GitHub Actions OIDC token minting for pub.dev trusted publishing.
//!
//! `dart-lang/setup-dart` mints a pub.dev-scoped GitHub Actions OIDC token
//! once, during workflow setup, and registers it with
//! `dart pub token add https://pub.dev --env-var PUB_TOKEN`. GitHub OIDC
//! tokens expire after five minutes
//! (<https://github.com/actions/toolkit/issues/2048>) while multi-ecosystem
//! publish runs may reach the dart step long after setup, at which point
//! pub.dev rejects the expired JWT. These helpers re-implement the
//! setup-dart minting flow so the token can be minted immediately before
//! each `dart pub publish` invocation instead.

use std::collections::BTreeMap;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use reqwest::Client;
use serde_json::Value as JsonValue;

pub const ACTIONS_ID_TOKEN_REQUEST_URL_ENV: &str = "ACTIONS_ID_TOKEN_REQUEST_URL";
pub const ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV: &str = "ACTIONS_ID_TOKEN_REQUEST_TOKEN";
/// Audience (`aud` claim) pub.dev requires on GitHub Actions OIDC tokens.
pub const PUB_DEV_AUDIENCE: &str = "https://pub.dev";
/// Environment variable the pub client reads the pub.dev bearer token from.
pub const PUB_TOKEN_ENV_VAR: &str = "PUB_TOKEN";

/// GitHub Actions OIDC endpoint context read from the ambient environment.
///
/// The runner only injects `ACTIONS_ID_TOKEN_REQUEST_URL` and
/// `ACTIONS_ID_TOKEN_REQUEST_TOKEN` when the workflow job grants
/// `permissions: id-token: write`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActionsOidcContext {
	/// Token endpoint exposed by the runner (`ACTIONS_ID_TOKEN_REQUEST_URL`).
	pub request_url: String,
	/// Bearer token authenticating requests to `request_url`
	/// (`ACTIONS_ID_TOKEN_REQUEST_TOKEN`).
	pub request_token: String,
}

#[must_use = "the OIDC context must be used to mint an ID token"]
/// Detect the GitHub Actions OIDC endpoint context from `env_map`.
///
/// Returns `None` when the job does not provide both endpoint variables with
/// non-empty values, which covers local runs and workflows without
/// `permissions: id-token: write`.
pub fn actions_oidc_context(env_map: &BTreeMap<String, String>) -> Option<ActionsOidcContext> {
	let request_url = env_map.get(ACTIONS_ID_TOKEN_REQUEST_URL_ENV)?.trim();
	if request_url.is_empty() {
		return None;
	}
	let request_token = env_map.get(ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV)?.trim();
	if request_token.is_empty() {
		return None;
	}
	Some(ActionsOidcContext {
		request_url: request_url.to_string(),
		request_token: request_token.to_string(),
	})
}

#[must_use = "the request URL must be used to mint an ID token"]
/// Build the GitHub Actions OIDC request URL for `audience`.
///
/// Mirrors `@actions/core.getIDToken`, which appends
/// `&audience=<url-encoded audience>` to `ACTIONS_ID_TOKEN_REQUEST_URL`.
pub fn actions_id_token_request_url(request_url: &str, audience: &str) -> String {
	format!("{request_url}&audience={}", urlencoding::encode(audience))
}

/// Mint a fresh GitHub Actions OIDC token for the pub.dev audience.
///
/// Performs the same exchange as setup-dart
/// (`@actions/core.getIDToken("https://pub.dev")`): a GET against the
/// runner-provided token endpoint with bearer authentication, returning the
/// JWT from the JSON `value` field.
pub async fn mint_actions_id_token(
	client: &Client,
	context: &ActionsOidcContext,
) -> MonochangeResult<String> {
	let request_url = actions_id_token_request_url(&context.request_url, PUB_DEV_AUDIENCE);
	let response = client
		.get(&request_url)
		.bearer_auth(&context.request_token)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Io(format!(
				"GitHub Actions OIDC token request for {request_url} failed: {error}"
			))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Io(format!(
			"GitHub Actions OIDC token request for {request_url} failed with HTTP {}: the workflow job must grant `permissions: id-token: write`",
			response.status()
		)));
	}

	let payload: JsonValue = response.json().await.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to decode the GitHub Actions OIDC response from {request_url}: {error}"
		))
	})?;
	payload
		.get("value")
		.and_then(JsonValue::as_str)
		.map(str::to_string)
		.ok_or_else(|| {
			MonochangeError::Io(format!(
				"the GitHub Actions OIDC response from {request_url} does not contain a `value` token field"
			))
		})
}

#[cfg(test)]
#[path = "__tests__/oidc_tests.rs"]
mod tests;
