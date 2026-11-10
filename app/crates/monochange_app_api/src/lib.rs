//! API layer — session management, JWT auth, middleware.
//!
//! Provides:
//! - JWT token creation and verification
//! - Session cookie management
//! - GitHub App authentication, webhook verification, and OIDC verification
//! - Hosted release commit and release request endpoints
//! - The public contributor task API

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

pub mod client_auth;
mod config;
pub mod embedded_api;
pub mod feedback;
pub mod github_app;
pub mod oauth;
pub mod oidc;
pub mod release_api;
pub mod sessions;
pub mod tasks_api;

pub mod token_vault;
pub mod webhooks;
pub mod workflow_api;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
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

/// Typed secrets generated from `app/monosecret.toml` at compile time.
pub mod secrets {
	// patch-coverage:ignore-start -- macro-generated loaders and setters are attributed to this line; `load_app_secrets` tests exercise the typed `load()` path the app uses.
	monosecret_derive::declare_secrets!("../../monosecret.toml");
	// patch-coverage:ignore-end
}

pub use secrets::Monosecret as AppSecrets;

/// The access reason recorded in Monosecret's audit log, and the one that
/// satisfies the manifest's `require_reason` policy.
const APP_SECRETS_REASON: &str = "start the monochange.dev server";

/// Load application secrets through the Monosecret SDK, recording the server
/// start as the access reason.
///
/// Pass `AppSecrets::builder()` to resolve the profile the environment selects:
/// Monosecret finds `monosecret.toml` by walking up from the working directory
/// and reads the profile from `MONOSECRET_PROFILE`. Each profile routes its own
/// secrets, so production reads the shared 1Password item while development
/// and CI stay on local sources. Tests pin a profile and a local store on the
/// builder instead.
pub fn load_app_secrets(
	builder: secrets::MonosecretBuilder,
) -> Result<monosecret::Resolved<AppSecrets>, monosecret::MonosecretError> {
	builder.with_reason(APP_SECRETS_REASON).load()
}

/// Largest hosted release commit request body the app accepts.
///
/// A request that uploads file contents carries the whole text of every file
/// the release changes; monochange's own grouped release is about 3 MiB. The
/// body is only read after the caller's credentials are verified.
pub const RELEASE_COMMIT_BODY_LIMIT: usize = 8 * 1024 * 1024;

/// Largest hosted release pull request body the app accepts, matching
/// axum's default.
pub const RELEASE_REQUEST_BODY_LIMIT: usize = 2 * 1024 * 1024;

/// The machine-to-machine API routes served by the app.
///
/// The router owns its state so it can be merged into the Leptos router
/// without coupling the bot endpoints to the SSR state type.
pub fn api_router(state: AppState) -> Router<()> {
	Router::new()
		.merge(embedded_api::iframe_router())
		.merge(client_auth::handoff_router())
		.merge(
			embedded_api::router().layer(axum::middleware::from_fn_with_state(
				state.clone(),
				embedded_api::cors,
			)),
		)
		.route("/api/github/webhooks", post(webhooks::github_webhook))
		.route(
			"/api/release-commits",
			post(release_api::create_release_commit),
		)
		.route(
			"/api/release-requests",
			post(release_api::publish_release_request),
		)
		.route(
			"/api/workflow-dispatches",
			post(workflow_api::dispatch_workflow),
		)
		.route(
			"/api/projects/{organization}/{project}/tasks",
			get(tasks_api::list_tasks),
		)
		.route(
			"/api/projects/{organization}/{project}/tasks/{task}",
			get(tasks_api::show_task),
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
	/// The user's `session_epoch` when this token was minted. Old tokens
	/// without the field deserialize as `0`, so they stay valid until the
	/// first global sign-out increments the epoch.
	#[serde(default)]
	pub epoch: i64,
}

/// Create a JWT for a user session at the user's current `session_epoch`.
pub fn create_token(
	secret: &str,
	user_id: i32,
	github_id: i64,
	github_login: &str,
	epoch: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
	let now = Utc::now();
	let exp = now + Duration::days(7);

	let claims = Claims {
		sub: user_id,
		github_id,
		github_login: github_login.to_string(),
		exp: exp.timestamp() as usize,
		iat: now.timestamp() as usize,
		epoch,
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
	/// Optional deployment-configured model; tests may inject a mock provider.
	pub feedback_assistant: Option<embedded_api::AssistantConfig>,
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
	/// Verifier for GitHub Actions OIDC tokens, shared so the JWKS it fetches
	/// is cached across requests.
	pub oidc: Arc<oidc::OidcVerifier>,
	/// `MONOCHANGE_TOKEN` accepted by the hosted release endpoints; `None`
	/// requires GitHub Actions OIDC.
	pub api_token: Option<String>,
	/// Repositories the `MONOCHANGE_TOKEN` may act on, from
	/// `MONOCHANGE_TOKEN_REPOSITORIES` (`owner/repo` entries separated by
	/// commas or whitespace, compared case-insensitively). `None` leaves the
	/// token unable to act anywhere, so the shared token never becomes a
	/// deployment-wide credential by accident.
	pub api_token_repositories: Option<std::collections::HashSet<String>>,
	/// Seals GitHub OAuth tokens before they reach the database.
	pub token_vault: Arc<token_vault::TokenVault>,
	/// Shared HTTP client for GitHub API calls.
	pub http: reqwest::Client,
	/// Serialize refresh-token rotation so one browser session cannot invalidate another request.
	pub github_token_refresh_lock: Arc<tokio::sync::Mutex<()>>,
	/// Bound provider work even when callbacks arrive from many addresses.
	pub oauth_exchange_slots: Arc<tokio::sync::Semaphore>,
	/// Exact redirect URIs the native handoff may return to.
	pub native_redirect_uris: Vec<String>,
	/// Exact redirect URIs the extension handoff may return to. Empty until an
	/// extension id is configured, so an unconfigured extension cannot connect.
	pub extension_redirect_uris: Vec<String>,
}

/// Parse a comma- or whitespace-separated allowlist, falling back to `default`
/// when the configured value is missing or empty.
fn parse_allowlist(raw: Option<String>, default: &[&str]) -> Vec<String> {
	raw.filter(|value| !value.trim().is_empty())
		.map(|value| {
			value
				.split([',', ' ', '\n', '\t'])
				.map(str::trim)
				.filter(|entry| !entry.is_empty())
				.map(str::to_string)
				.collect::<Vec<_>>()
		})
		.filter(|values| !values.is_empty())
		.unwrap_or_else(|| default.iter().map(|entry| (*entry).to_string()).collect())
}

impl AppState {
	/// Initialize the shared state, rejecting incomplete or invalid bot credentials.
	pub fn new(
		db: monochange_app_db::DbPool,
		secrets: AppSecrets,
	) -> Result<Self, github_app::GitHubAppError> {
		let jwt_secret = secrets.jwt_secret.clone();
		// An empty string is a valid HMAC key, so a profile that resolves
		// JWT_SECRET to nothing would sign forgeable session tokens; the
		// required-secrets check cannot catch a resolved-but-empty value.
		if jwt_secret.is_empty() {
			return Err(github_app::GitHubAppError::EmptyJwtSecret);
		}

		let github_client_id = secrets.github_client_id.clone();
		let github_client_secret = secrets.github_client_secret.clone();
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
		let api_token = secrets
			.monochange_token
			.clone()
			.filter(|token| !token.is_empty());
		let api_token_repositories = std::env::var("MONOCHANGE_TOKEN_REPOSITORIES")
			.ok()
			.and_then(|raw| {
				let repositories = raw
					.split([',', ' ', '\n', '\t'])
					.map(str::trim)
					.filter(|entry| !entry.is_empty())
					.map(str::to_lowercase)
					.collect::<std::collections::HashSet<_>>();
				(!repositories.is_empty()).then_some(repositories)
			});
		let token_vault = token_vault::TokenVault::new(&secrets.token_encryption_key)
			.map_err(|_| github_app::GitHubAppError::EmptyTokenEncryptionKey)?;
		// GitHub rejects API requests without a User-Agent header with 403.
		let http = reqwest::Client::builder()
			.user_agent("monochange")
			.connect_timeout(std::time::Duration::from_secs(10))
			.timeout(std::time::Duration::from_secs(30))
			.build()?;

		Ok(Self {
			feedback_assistant: embedded_api::AssistantConfig::from_environment(
				secrets.openrouter_api_key.as_deref(),
			),
			db,
			secrets: Arc::new(secrets),
			jwt_secret,
			github_client_id,
			github_client_secret,
			github_oauth_origin: "https://github.com".to_string(),
			github_callback_url,
			github_api_origin: "https://api.github.com".to_string(),
			github_app,
			oidc: Arc::new(oidc::OidcVerifier::new(&oidc_audience)),
			oidc_audience,
			api_token,
			api_token_repositories,
			token_vault: Arc::new(token_vault),
			http,
			github_token_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
			oauth_exchange_slots: Arc::new(tokio::sync::Semaphore::new(16)),
			native_redirect_uris: parse_allowlist(
				std::env::var("MONOCHANGE_NATIVE_REDIRECT_URIS").ok(),
				&[client_auth::NATIVE_REDIRECT_DEFAULT],
			),
			extension_redirect_uris: parse_allowlist(
				std::env::var("MONOCHANGE_EXTENSION_REDIRECT_URIS").ok(),
				&[],
			),
		})
	}
}

impl AppState {
	/// The site's own origin, used to check same-origin handoff requests.
	#[must_use]
	pub fn site_origin(&self) -> Option<String> {
		url::Url::parse(&self.github_callback_url)
			.ok()
			.map(|url| url.origin().ascii_serialization())
	}
}
