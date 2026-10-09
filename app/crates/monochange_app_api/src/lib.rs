//! API layer — session management, JWT auth, middleware.
//!
//! Provides:
//! - JWT token creation and verification
//! - Session cookie management
//! - GitHub App authentication, webhook verification, and OIDC verification
//! - Hosted release commit and release request endpoints

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

mod config;
pub mod github_app;
pub mod oauth;
pub mod oidc;
pub mod release_api;
pub mod webhooks;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::post;
use chrono::Duration;
use chrono::Utc;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use jsonwebtoken::decode;
use jsonwebtoken::encode;
use serde::Deserialize;
use serde::Serialize;

// The `declare_secrets!` macro derives `serde::Deserialize` on generated types
// whose accessors use unsafe; the lint must be allowed at module scope because
// attributes on macro invocations are ignored.
#[allow(clippy::unsafe_derive_deserialize)]
pub mod secrets {
	secretspec_derive::declare_secrets!("../../secretspec.toml");
}

pub use secrets::SecretSpec as AppSecrets;

/// Load application secrets through the `SecretSpec` SDK.
pub fn load_app_secrets() -> Result<secretspec::Resolved<AppSecrets>, secretspec::SecretSpecError> {
	secrets::SecretSpec::builder().load()
}

/// The machine-to-machine API routes served by the app.
///
/// The router owns its state so it can be merged into the Leptos router
/// without coupling the bot endpoints to the SSR state type.
pub fn api_router(state: AppState) -> Router<()> {
	Router::new()
		.route("/api/github/webhooks", post(webhooks::github_webhook))
		.route(
			"/api/release-commits",
			post(release_api::create_release_commit),
		)
		.route(
			"/api/release-requests",
			post(release_api::publish_release_request),
		)
		.with_state(state)
}

/// JWT claims stored in the session token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
	/// Subject = user's DB id
	pub sub: i32,
	/// GitHub user ID
	pub github_id: i64,
	/// GitHub login
	pub github_login: String,
	/// Expiration timestamp
	pub exp: usize,
	/// Issued at timestamp
	pub iat: usize,
}

/// Create a JWT for a user session.
pub fn create_token(
	secret: &str,
	user_id: i32,
	github_id: i64,
	github_login: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
	let now = Utc::now();
	let exp = now + Duration::days(7);

	let claims = Claims {
		sub: user_id,
		github_id,
		github_login: github_login.to_string(),
		exp: exp.timestamp() as usize,
		iat: now.timestamp() as usize,
	};

	encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret.as_bytes()),
	)
}

/// Verify a JWT and extract claims.
pub fn verify_token(secret: &str, token: &str) -> Result<Claims, StatusCode> {
	decode::<Claims>(
		token,
		&DecodingKey::from_secret(secret.as_bytes()),
		&Validation::default(),
	)
	.map(|data| data.claims)
	.map_err(|_| StatusCode::UNAUTHORIZED)
}

/// Application state shared across requests.
#[derive(Clone)]
pub struct AppState {
	pub db: monochange_app_db::DbPool,
	pub secrets: Arc<AppSecrets>,
	pub jwt_secret: String,
	pub github_client_id: String,
	pub github_client_secret: String,
	/// GitHub's OAuth web origin, separated from the API origin.
	pub github_oauth_origin: String,
	/// Exact callback registered for GitHub App user authorization.
	pub github_callback_url: String,
	/// GitHub API origin used for authenticated user requests.
	pub github_api_origin: String,
	/// GitHub App credentials for the monochange bot; `None` in development.
	pub github_app: Option<github_app::GitHubAppAuth>,
	/// OIDC audience required in GitHub Actions tokens.
	pub oidc_audience: String,
	/// Shared HTTP client for GitHub API calls.
	pub http: reqwest::Client,
	/// Serialize refresh-token rotation so one browser session cannot invalidate another request.
	pub github_token_refresh_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
	/// Initialize the shared state, rejecting incomplete or invalid bot credentials.
	pub fn new(
		db: monochange_app_db::DbPool,
		secrets: AppSecrets,
	) -> Result<Self, github_app::GitHubAppError> {
		let jwt_secret = secrets.jwt_secret.clone().unwrap_or_default();
		let github_client_id = secrets.github_client_id.clone().unwrap_or_default();
		let github_client_secret = secrets.github_client_secret.clone().unwrap_or_default();
		let github_callback_url = std::env::var("MONOCHANGE_GITHUB_CALLBACK_URL")
			.ok()
			.filter(|url| !url.is_empty())
			.unwrap_or_else(|| "https://monochange.dev/auth/callback".to_string());
		let github_app = config::github_app_credentials(&secrets)?;
		let oidc_audience = std::env::var("MONOCHANGE_OIDC_AUDIENCE")
			.ok()
			.filter(|audience| !audience.is_empty())
			.or_else(|| {
				secrets
					.monochange_oidc_audience
					.clone()
					.filter(|audience| !audience.is_empty())
			})
			.unwrap_or_else(|| "monochange.dev".to_string());
		let http = reqwest::Client::builder()
			.connect_timeout(std::time::Duration::from_secs(10))
			.timeout(std::time::Duration::from_secs(30))
			.build()?;

		Ok(Self {
			db,
			secrets: Arc::new(secrets),
			jwt_secret,
			github_client_id,
			github_client_secret,
			github_oauth_origin: "https://github.com".to_string(),
			github_callback_url,
			github_api_origin: "https://api.github.com".to_string(),
			github_app,
			oidc_audience,
			http,
			github_token_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
		})
	}
}
