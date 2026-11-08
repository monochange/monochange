#![allow(clippy::disallowed_methods)]

use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;

async fn test_state() -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.expect("open test database");
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "backend-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.expect("build test secrets");
	AppState::new(db, secrets).expect("construct app state")
}

#[test]
fn machine_api_router_symbol_matches_documented_signature() {
	let mount: fn(monochange_app_api::AppState) -> axum::Router<()> = crate::machine_api_router;
	let reexport: fn(monochange_app_api::AppState) -> axum::Router<()> = crate::api_router;
	assert_eq!(
		std::mem::size_of_val(&mount),
		std::mem::size_of_val(&reexport)
	);
}

#[test]
fn api_router_reexport_is_app_api_router() {
	let via_backend: fn(monochange_app_api::AppState) -> axum::Router<()> = crate::api_router;
	let via_api: fn(monochange_app_api::AppState) -> axum::Router<()> =
		monochange_app_api::api_router;
	assert_eq!(
		std::any::type_name_of_val(&via_backend),
		std::any::type_name_of_val(&via_api)
	);
}

#[tokio::test]
async fn machine_api_router_forwards_to_app_api_router() {
	let state = test_state().await;
	let _router = crate::machine_api_router(state);
}

#[tokio::test]
async fn api_router_reexport_builds_the_same_mount() {
	let state = test_state().await;
	let _router = crate::api_router(state);
}
