//! Browser-only session management and the device-token gate.
//!
//! `connect_request`, `list_client_sessions`, `revoke_client_session`, and
//! `sign_out_everywhere` require the browser's host-only cookie. Native and
//! extension tokens can read the shared project surface but cannot mint
//! tokens, manage devices, or exchange OAuth.

#[cfg(test)]
#[path = "__tests__/session_tests.rs"]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use leptos::prelude::*;
use leptos::server;
use leptoskit_rpc::api;
#[cfg(not(target_arch = "wasm32"))]
use monochange_app_api::AppState;
#[cfg(not(target_arch = "wasm32"))]
use monochange_app_api::client_auth;
#[cfg(not(target_arch = "wasm32"))]
use monochange_app_api::sessions::Principal;
#[cfg(not(target_arch = "wasm32"))]
use monochange_app_db::client_sessions;
#[cfg(not(target_arch = "wasm32"))]
use monochange_shared::ClientKind;
use monochange_shared::ClientSessionInfo;
use monochange_shared::ConnectPreview;
#[cfg(not(target_arch = "wasm32"))]
use monochange_shared::is_valid_code_challenge;
#[cfg(not(target_arch = "wasm32"))]
use monochange_shared::is_valid_state;

/// The consent preview for a browser handoff, or `None` when the client,
/// redirect, challenge, or state is not acceptable.
#[api(crate = "leptoskit_rpc", tag = "session")]
#[server(endpoint = "connect_request")]
pub async fn connect_request(
	client: String,
	redirect_uri: String,
	code_challenge: String,
	state: String,
) -> Result<Option<ConnectPreview>, server_fn::ServerFnError> {
	let user = require_browser().await?;
	let state_context = expect_context::<Arc<AppState>>();
	let client_kind = ClientKind::parse(&client)
		.ok_or_else(|| server_fn::ServerFnError::new("Unsupported client"))?;

	if !client_auth::redirect_allowed(&state_context, client_kind, &redirect_uri)
		|| !is_valid_code_challenge(&code_challenge)
		|| !is_valid_state(&state)
	{
		return Ok(None);
	}

	Ok(Some(ConnectPreview {
		user: user.session_user(),
		client: client_kind,
	}))
}

/// The browser user's own client sessions, without bearer tokens.
#[api(crate = "leptoskit_rpc", tag = "session")]
#[server(endpoint = "list_client_sessions")]
pub async fn list_client_sessions() -> Result<Vec<ClientSessionInfo>, server_fn::ServerFnError> {
	let state = expect_context::<Arc<AppState>>();
	let user = require_browser().await?;

	let sessions = client_sessions::list_sessions(&state.db, user.user_id)
		.await
		.map_err(|_| server_fn::ServerFnError::new("Session list unavailable"))?;

	Ok(sessions
		.into_iter()
		.filter_map(|session| {
			let client = ClientKind::parse(&session.client)?;
			Some(ClientSessionInfo {
				id: session.id,
				client,
				created_at: session.created_at,
				last_used_at: session.last_used_at,
				expires_at: session.expires_at,
			})
		})
		.collect())
}

/// Revoke one of the browser user's own client sessions.
#[api(crate = "leptoskit_rpc", tag = "session")]
#[server(endpoint = "revoke_client_session")]
pub async fn revoke_client_session(id: String) -> Result<(), server_fn::ServerFnError> {
	let state = expect_context::<Arc<AppState>>();
	let user = require_browser().await?;
	require_same_origin().await?;

	client_sessions::revoke_session_by_id(&state.db, user.user_id, &id)
		.await
		.map_err(|_| server_fn::ServerFnError::new("Session revocation unavailable"))?;

	Ok(())
}

/// Increment the user's epoch, revoke every client grant and session, and
/// clear the browser cookie. The current browser session stops resolving.
#[api(crate = "leptoskit_rpc", tag = "session")]
#[server(endpoint = "sign_out_everywhere")]
pub async fn sign_out_everywhere() -> Result<(), server_fn::ServerFnError> {
	use leptos_axum::ResponseOptions;

	let state = expect_context::<Arc<AppState>>();
	let user = require_browser().await?;
	require_same_origin().await?;

	client_sessions::revoke_all_for_user(&state.db, user.user_id)
		.await
		.map_err(|_| server_fn::ServerFnError::new("Sign-out unavailable"))?;

	let response = expect_context::<ResponseOptions>();
	let cookie = monochange_app_api::oauth::session_cookie(String::new());
	response.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn require_browser()
-> Result<monochange_app_api::sessions::BrowserPrincipal, server_fn::ServerFnError> {
	match super::principal().await? {
		Principal::Browser(user) => Ok(user),
		_ => Err(server_fn::ServerFnError::new("Authentication required")),
	}
}

/// State-changing browser calls must come from the site's own origin.
#[cfg(not(target_arch = "wasm32"))]
async fn require_same_origin() -> Result<(), server_fn::ServerFnError> {
	use leptos_axum::extract;

	let state = expect_context::<Arc<AppState>>();
	let headers: axum::http::HeaderMap = extract().await?;
	let origin = state.site_origin();
	let same_origin = headers
		.get(axum::http::header::ORIGIN)
		.and_then(|value| value.to_str().ok())
		.zip(origin.as_deref())
		.is_some_and(|(origin, expected)| origin == expected);

	if !same_origin {
		return Err(server_fn::ServerFnError::new(
			"Cross-origin request rejected",
		));
	}

	Ok(())
}

/// Server function paths a client token may call. Everything else is
/// browser-only.
#[cfg(not(target_arch = "wasm32"))]
const CLIENT_ALLOWED_PATHS: &[&str] = &[
	<crate::server_fns::auth::GetSession as server_fn::ServerFn>::PATH,
	<crate::server_fns::organizations::ListOrganizations as server_fn::ServerFn>::PATH,
	<crate::server_fns::organizations::OrganizationOverview as server_fn::ServerFn>::PATH,
	<crate::server_fns::organizations::ProjectOverview as server_fn::ServerFn>::PATH,
	<crate::server_fns::releases::GetWebsiteReleases as server_fn::ServerFn>::PATH,
];

/// Reject a client token that targets a browser-only path. Mounted after the
/// principal middleware, which stores the resolved principal in the request.
#[cfg(not(target_arch = "wasm32"))]
pub async fn bearer_gate(
	request: axum::extract::Request,
	next: axum::middleware::Next,
) -> axum::response::Response {
	use axum::http::StatusCode;

	let path = request.uri().path().to_owned();
	let is_server_fn = server_fn_paths().contains(path.as_str());
	let is_handoff = path == client_auth::TOKEN_PATH
		|| path == client_auth::REVOKE_PATH
		|| path == client_auth::APPROVE_PATH;

	if !is_server_fn && !is_handoff {
		return next.run(request).await;
	}

	let Some(principal) = request.extensions().get::<Principal>().cloned() else {
		return gate_rejection(
			StatusCode::INTERNAL_SERVER_ERROR,
			"authorization unavailable",
			is_server_fn,
		);
	};

	if !matches!(principal, Principal::Client(_)) {
		return next.run(request).await;
	}

	let allowed = CLIENT_ALLOWED_PATHS.contains(&path.as_str()) || path == client_auth::REVOKE_PATH;
	if allowed {
		return next.run(request).await;
	}

	gate_rejection(
		StatusCode::FORBIDDEN,
		"client credentials cannot use this endpoint",
		is_server_fn,
	)
}

#[cfg(not(target_arch = "wasm32"))]
fn server_fn_paths() -> &'static std::collections::HashSet<&'static str> {
	static PATHS: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
		std::sync::LazyLock::new(|| {
			server_fn::axum::server_fn_paths()
				.map(|(path, _)| path)
				.collect()
		});
	&PATHS
}

#[cfg(not(target_arch = "wasm32"))]
fn gate_rejection(
	status: axum::http::StatusCode,
	message: &str,
	is_server_fn: bool,
) -> axum::response::Response {
	use axum::response::IntoResponse;
	use server_fn::error::FromServerFnError;

	if is_server_fn {
		let parts = server_fn::ServerFnError::new(message.to_string()).ser();
		let mut response = (
			[(axum::http::header::CONTENT_TYPE, parts.content_type)],
			axum::body::Body::from(parts.body),
		)
			.into_response();
		*response.status_mut() = status;
		response
	} else {
		(status, axum::Json(serde_json::json!({ "error": message }))).into_response()
	}
}
