//! `monochange_app` server binary.
//!
//! Starts an axum HTTP server with Leptos SSR integration for both browser
//! frontends (the website entry and the projects entry), a `SQLite`
//! connection pool, and JWT session management.
//!
//! ## Secret management
//!
//! Secrets are declared in `monosecret.toml` and loaded in-process via the
//! Monosecret Rust SDK. `MONOSECRET_PROFILE` selects the profile, and each
//! profile routes its own secrets: production reads one 1Password item with
//! the service account token that Docker injects as a Docker secret, while
//! development and CI never contact 1Password.
//!
//! ```bash
//! # Development (reads app/.env, then the environment, then local defaults)
//! # Prefer: MONOSECRET_PROFILE=development leptoskit serve  (from app/)
//! MONOSECRET_PROFILE=development leptoskit serve
//!
//! # Production (the container entrypoint exports OP_SERVICE_ACCOUNT_TOKEN and
//! # the app reads the `monochange.dev` item in the `monochange` vault)
//! MONOSECRET_PROFILE=production ./monochange_app
//!
//! # CI (reads environment variables only)
//! MONOSECRET_PROFILE=ci ./monochange_app
//! ```

// The server also computes layouts for Leptos' generated view types.
#![recursion_limit = "256"]

use std::sync::Arc;

use leptos::prelude::get_configuration;
use monochange_app_api::AppState;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Application-level error type.
#[derive(Debug)]
pub enum MonochangeError {
	/// Database connection or migration failure.
	Database(String),
	/// Server initialization failure.
	Server(String),
}

impl std::fmt::Display for MonochangeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Database(msg) => write!(f, "database error: {msg}"),
			Self::Server(msg) => write!(f, "server error: {msg}"),
		}
	}
}

impl std::error::Error for MonochangeError {}

// `#[tokio::main]` generates `Builder::expect("Failed building the Runtime")`
// internally; this function never calls `Result::expect` directly.
#[allow(clippy::disallowed_methods)]
#[tokio::main]
async fn main() -> Result<(), MonochangeError> {
	// Initialize tracing
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env()
				.unwrap_or_else(|_| EnvFilter::new("monochange_app=debug,info")),
		)
		.init();

	// Check the compiled book before accepting traffic or loading credentials.
	monochange_app::book::compiled_book().map_err(MonochangeError::Server)?;

	// Load typed application secrets through the Monosecret SDK. The
	// production profile reads the shared 1Password item with the service
	// account token the Docker entrypoint exports from a Docker secret.
	// patch-coverage:ignore-start -- binary startup is exercised by app/deploy/verify-image.sh; loading is covered by monochange_app_api tests.
	let resolved_secrets =
		monochange_app_api::load_app_secrets(monochange_app_api::AppSecrets::builder())
			.map_err(|error| MonochangeError::Server(error.to_string()))?;
	let secrets = resolved_secrets.secrets;
	let database_url = secrets.database_url.clone();
	// patch-coverage:ignore-end
	// Create database connection pool
	info!("Connecting to database...");
	let pool = monochange_app_db::create_pool(&database_url)
		.await
		.map_err(|e| MonochangeError::Database(e.to_string()))?;

	// Run database migrations (skip if already applied)
	info!("Running migrations...");
	monochange_app_db::run_migrations(&pool)
		.await
		.map_err(|e| MonochangeError::Database(e.to_string()))?;
	info!("Database ready");

	let automation_config = monochange_app_automation::AutomationRuntimeConfig::from_env()
		.map_err(|error| MonochangeError::Server(error.to_string()))?;
	let _automation_worker: Option<tokio::task::JoinHandle<()>> =
		monochange_app_automation::spawn_sqlite_automation_worker(pool.clone(), automation_config);

	// Create application state
	let app_state = Arc::new(
		AppState::new(pool, secrets).map_err(|error| MonochangeError::Server(error.to_string()))?,
	);

	// Limit the API per client: signed-in users by user id, everyone else by
	// address. Server function paths reject in the Leptos error encoding.
	let rate_limit = monochange_app::rate_limit::RateLimit::new(
		monochange_app::rate_limit::RateLimitConfig::from_env()
			.map_err(|error| MonochangeError::Server(error.to_string()))?,
		app_state.jwt_secret.clone(),
		server_fn::axum::server_fn_paths().map(|(path, _)| path),
	);
	let _rate_limit_cleanup =
		rate_limit.spawn_cleanup(monochange_app::rate_limit::CLEANUP_INTERVAL);

	// Leptos configuration: the site root and pkg layout the leptoskit build
	// assembled, shared by both browser frontends.
	let conf = get_configuration(None).map_err(|e| MonochangeError::Server(e.to_string()))?;
	let leptos_options = conf.leptos_options.clone();

	// One router composes both browser frontends behind the same backend:
	// the website entry and the projects entry. Listings merge before
	// registration, so the server-function `/api` routes register once.
	let app = monochange_app::router::build_router(&app_state, leptos_options.clone(), rate_limit);

	let addr = std::env::var("PORT")
		.ok()
		.and_then(|p| format!("0.0.0.0:{p}").parse().ok())
		.unwrap_or(leptos_options.site_addr);
	info!("monochange_app starting on http://{addr}");

	let listener = tokio::net::TcpListener::bind(&addr)
		.await
		.map_err(|e| MonochangeError::Server(e.to_string()))?;
	// Anonymous clients are rate limited by address, which needs the peer.
	leptoskit::server::axum::serve(
		listener,
		app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
	)
	.await
	.map_err(|e| MonochangeError::Server(e.to_string()))?;

	Ok(())
}
