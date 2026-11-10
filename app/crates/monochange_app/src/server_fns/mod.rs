//! Server functions — product UI RPCs that run on the server.
//!
//! These use Leptos `#[server]` (and, as the leptoskit port lands,
//! `leptoskit_rpc` `#[api]`) so the same signatures compile to client-side
//! fetch calls and server-side implementations.
//!
//! Wire DTOs that extension/native guests need live in `monochange_shared`
//! and are re-exported from these modules so existing imports keep working.
//! Machine webhooks / OIDC / embed SDK routes stay on Axum via
//! `monochange_backend::machine_api_router`.

pub mod ai;
pub mod auth;
pub mod book;
pub mod embedded;
pub mod feedback;
pub mod installation;
pub mod organizations;
pub mod portal;

pub mod releases;

pub mod repos;
pub mod session;

/// The request's resolved principal, from the auth middleware's request
/// extension or, when a server function runs outside that middleware, the
/// request's own credentials.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn principal()
-> Result<monochange_app_api::sessions::Principal, server_fn::ServerFnError> {
	use std::sync::Arc;

	use axum::http::header::AUTHORIZATION;
	use leptos_axum::extract;

	if let Some(axum::Extension(principal)) =
		extract::<Option<axum::Extension<monochange_app_api::sessions::Principal>>>().await?
	{
		return Ok(principal);
	}

	let headers: axum::http::HeaderMap = extract().await?;
	let cookies = axum_extra::extract::cookie::CookieJar::from_headers(&headers);

	// A request with no credentials is anonymous without touching the state;
	// a presented credential always needs it, and its absence is an error.
	if !headers.contains_key(AUTHORIZATION)
		&& cookies
			.get(monochange_app_api::oauth::SESSION_COOKIE_NAME)
			.is_none()
	{
		return Ok(monochange_app_api::sessions::Principal::Anonymous);
	}

	let state = leptos::prelude::expect_context::<Arc<monochange_app_api::AppState>>();

	monochange_app_api::sessions::resolve_parts(&state, &headers, &cookies)
		.await
		.map_err(|error| server_fn::ServerFnError::new(format!("DB: {error}")))
}
