//! The production router serves each frontend behind its own entry bundle,
//! keeps legacy product redirects, and never mixes the two route trees.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::Request;
use axum::http::StatusCode;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use tower::ServiceExt;

async fn html(response: axum::response::Response) -> String {
	let bytes = tokio::time::timeout(
		Duration::from_secs(10),
		to_bytes(response.into_body(), 2_000_000),
	)
	.await
	.expect("SSR must finish its response stream")
	.unwrap();
	String::from_utf8(bytes.to_vec()).unwrap()
}

/// Minimal production state: an in-memory database and dummy test
/// credentials only; no secret access and no network calls.
async fn state() -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "router-test-signing-key",
		"token_encryption_key": "router-test-token-encryption-key",
		"github_client_id": "router-test-client",
		"github_client_secret": "router-test-client-secret"
	}))
	.unwrap();
	Arc::new(AppState::new(db, secrets).unwrap())
}

fn production_router(state: &Arc<AppState>) -> Router {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.site_root(
			std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../../target/site")
				.to_string_lossy()
				.into_owned(),
		)
		.build();
	let rate_limit = crate::rate_limit::RateLimit::new(
		crate::rate_limit::RateLimitConfig::default(),
		state.jwt_secret.clone(),
		server_fn::axum::server_fn_paths().map(|(path, _)| path),
	);
	crate::router::build_router(state, options, rate_limit)
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
	let response = app
		.clone()
		.oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
		.await
		.unwrap();
	let status = response.status();
	(status, html(response).await)
}

#[tokio::test]
async fn website_pages_hydrate_through_the_website_entry() {
	let app = production_router(&state().await);
	let (status, landing) = get(&app, "/").await;
	assert_eq!(status, StatusCode::OK);
	assert!(landing.contains("/pkg/site/monochange_app.js"), "{landing}");
	assert!(
		landing.contains("/pkg/site/monochange_app.css"),
		"{landing}"
	);
	assert!(
		!landing.contains("/pkg/projects/"),
		"website pages must not reference the projects bundle: {landing}"
	);
	assert!(landing.contains("monochange — Release planning for monorepos"));
}

#[tokio::test]
async fn product_pages_hydrate_through_the_projects_entry() {
	let app = production_router(&state().await);
	let (status, projects) = get(&app, "/projects").await;
	assert_eq!(status, StatusCode::OK);
	assert!(
		projects.contains("/pkg/projects/monochange_projects.js"),
		"{projects}"
	);
	assert!(
		projects.contains("/pkg/projects/monochange_projects.css"),
		"{projects}"
	);
	assert!(
		!projects.contains("/pkg/site/monochange_app.js"),
		"product pages must not load the website entry: {projects}"
	);
	// The projects entry renders its own navigation and no website footer.
	assert!(projects.contains("Projects navigation"), "{projects}");
	assert!(
		!projects.contains("aria-label=\"Footer navigation\""),
		"{projects}"
	);
}

#[tokio::test]
async fn feedback_portals_use_the_projects_entry_too() {
	let app = production_router(&state().await);
	let (status, portal) = get(&app, "/p/acme/web").await;
	assert_eq!(status, StatusCode::OK);
	assert!(
		portal.contains("/pkg/projects/monochange_projects.js"),
		"{portal}"
	);
}

#[tokio::test]
async fn the_repositories_dashboard_stays_on_the_website_entry() {
	let app = production_router(&state().await);
	let (status, dashboard) = get(&app, "/dashboard").await;
	assert_eq!(status, StatusCode::OK);
	assert!(
		dashboard.contains("/pkg/site/monochange_app.js"),
		"{dashboard}"
	);
}

#[tokio::test]
async fn paths_near_the_boundary_stay_on_the_website_entry() {
	let app = production_router(&state().await);
	// /pricing must not classify as the /p portal prefix.
	let (status, pricing) = get(&app, "/pricing").await;
	assert_eq!(status, StatusCode::OK);
	assert!(pricing.contains("/pkg/site/monochange_app.js"), "{pricing}");
}

#[tokio::test]
async fn legacy_dashboard_product_paths_permanently_redirect_to_projects() {
	let app = production_router(&state().await);
	let response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri("/dashboard/acme/projects/web/feedback")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
	assert_eq!(
		response.headers().get("location").unwrap(),
		"/projects/acme/projects/web/feedback"
	);
}

#[tokio::test]
async fn unknown_paths_render_the_website_error_page() {
	let app = production_router(&state().await);
	let (status, not_found) = get(&app, "/no-such-page").await;
	assert_eq!(status, StatusCode::NOT_FOUND);
	assert!(not_found.contains("status-page"), "{not_found}");
	assert!(
		not_found.contains("/pkg/site/monochange_app.js"),
		"the fallback shell hydrates the website entry: {not_found}"
	);
}

#[tokio::test]
async fn unmatched_product_paths_render_the_projects_error_page() {
	// More segments than any product route declares: the path dispatcher
	// keeps the request in the projects entry, whose router fallback renders
	// the shared error page with the projects bundle.
	let app = production_router(&state().await);
	let (status, body) = get(&app, "/projects/acme/web/extra/segments").await;
	assert_eq!(status, StatusCode::NOT_FOUND);
	assert!(body.contains("status-page"), "{body}");
	assert!(
		body.contains("/pkg/projects/monochange_projects.js"),
		"{body}"
	);
}

#[tokio::test]
async fn the_health_endpoint_reports_the_release_status() {
	let app = production_router(&state().await);
	let (status, health) = get(&app, "/health").await;
	assert_eq!(status, StatusCode::OK);
	assert!(health.contains("\"status\":\"ok\""), "{health}");
	assert!(health.contains("\"http\":\"up\""), "{health}");
}

#[tokio::test]
async fn server_function_routes_register_exactly_once() {
	// Merging both frontends' listings must not double-register the shared
	// `/api` endpoints: axum panics on overlap, and a duplicate-free router
	// still answers a shared server function path.
	let app = production_router(&state().await);
	let response = app
		.oneshot(
			Request::builder()
				.method(axum::http::Method::POST)
				.uri("/api/get_session")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	// A server function route exists (not 404/405 from a missing route);
	// without a session cookie it responds from the session server function.
	assert_ne!(response.status(), StatusCode::NOT_FOUND);
	assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
