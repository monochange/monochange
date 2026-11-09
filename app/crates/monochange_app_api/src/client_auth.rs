//! HTTP handlers for the browser handoff and opaque client tokens.
//!
//! The browser approves a connection through `/connect/approve`, which returns
//! only a one-time code. The native guest or extension exchanges that code at
//! `/api/client-sessions/token` for its own scoped credential. No token ever
//! travels in a redirect, every response is marked `no-store`, and the code is
//! burned before any exchange validation: a wrong verifier, client, or
//! redirect consumes the grant so a probe cannot retry it. Only a cross-site
//! browser `Origin` is rejected without burning, so a forged form post cannot
//! destroy a code that the real client is about to exchange.

use axum::Form;
use axum::Json;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::extract::Request;
use axum::extract::State;
use axum::extract::rejection::FormRejection;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use axum::routing::post;
use axum_extra::extract::cookie::CookieJar;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use monochange_app_db::client_sessions;
use monochange_shared::ClientKind;
use monochange_shared::ClientSession;
use monochange_shared::ClientTokenRequest;
use monochange_shared::SessionUser;
use monochange_shared::is_valid_code_challenge;
use monochange_shared::is_valid_code_verifier;
use monochange_shared::is_valid_state;
use serde::Deserialize;
use sha2::Digest;

use crate::AppState;
use crate::sessions::Principal;

/// JSON endpoint that exchanges a one-time code for a client session.
pub const TOKEN_PATH: &str = "/api/client-sessions/token";
/// JSON endpoint that revokes the caller's client session.
pub const REVOKE_PATH: &str = "/api/client-sessions/revoke";
/// Browser form endpoint that approves a connection and redirects with a code.
pub const APPROVE_PATH: &str = "/connect/approve";
/// Largest body the public handoff endpoints accept.
pub const HANDOFF_BODY_LIMIT: usize = 4096;
/// The default native return address, matching the upstream web-auth plugin.
pub const NATIVE_REDIRECT_DEFAULT: &str = "dev.monochange.app:/auth/native/return";

/// A handoff rejection. Every response, including failures, is `no-store`.
#[derive(Debug)]
pub struct HandoffError {
	status: StatusCode,
	message: &'static str,
}

impl HandoffError {
	pub fn new(status: StatusCode, message: &'static str) -> Self {
		Self { status, message }
	}

	fn invalid() -> Self {
		Self::new(StatusCode::BAD_REQUEST, "invalid handoff request")
	}

	fn unauthorized() -> Self {
		Self::new(StatusCode::UNAUTHORIZED, "authentication required")
	}

	fn forbidden() -> Self {
		Self::new(StatusCode::FORBIDDEN, "forbidden")
	}
}

impl IntoResponse for HandoffError {
	fn into_response(self) -> Response {
		let mut response = (
			self.status,
			Json(serde_json::json!({ "error": self.message })),
		)
			.into_response();
		response
			.headers_mut()
			.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
		response
	}
}

/// The public handoff routes, body-limited and always `no-store`.
pub fn handoff_router() -> Router<AppState> {
	Router::new()
		.route(TOKEN_PATH, post(exchange))
		.route(REVOKE_PATH, post(revoke))
		.route(APPROVE_PATH, post(approve))
		.layer(DefaultBodyLimit::max(HANDOFF_BODY_LIMIT))
		.layer(axum::middleware::from_fn(no_store))
}

async fn no_store(request: Request, next: Next) -> Response {
	let mut response = next.run(request).await;
	response
		.headers_mut()
		.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	response
}

/// Whether `redirect_uri` is registered for this client and has a safe shape.
#[must_use]
pub fn redirect_allowed(state: &AppState, client: ClientKind, redirect_uri: &str) -> bool {
	let configured = match client {
		ClientKind::Native => &state.native_redirect_uris,
		ClientKind::Extension => &state.extension_redirect_uris,
	};

	if !configured.iter().any(|uri| uri == redirect_uri) {
		return false;
	}

	match client {
		ClientKind::Native => leptoskit_plugin_web_auth::return_address(redirect_uri).is_ok(),
		ClientKind::Extension => extension_redirect_shape(redirect_uri).is_ok(),
	}
}

/// Extension redirects are exact `https` URLs with no query, fragment, or
/// userinfo. The allowlist is empty by default, so an extension cannot connect
/// until its redirect URI is configured.
fn extension_redirect_shape(redirect_uri: &str) -> Result<(), ()> {
	let url = url::Url::parse(redirect_uri).map_err(|_| ())?;

	if url.scheme() != "https"
		|| url.host().is_none()
		|| !url.username().is_empty()
		|| url.password().is_some()
		|| url.query().is_some()
		|| url.fragment().is_some()
	{
		return Err(());
	}

	Ok(())
}

/// The token endpoint rejects web `Origin` headers. Native sends none; any
/// present `Origin` must be the exact origin of a configured extension
/// redirect, so the site's own browser origin cannot call the handoff.
fn origin_allowed(state: &AppState, headers: &HeaderMap) -> bool {
	let Some(origin) = headers
		.get(header::ORIGIN)
		.and_then(|value| value.to_str().ok())
	else {
		return true;
	};

	state
		.extension_redirect_uris
		.iter()
		.filter_map(|uri| url::Url::parse(uri).ok())
		.any(|url| url.origin().ascii_serialization() == origin)
}

/// Exchange a one-time code for this client's opaque session.
pub async fn exchange(
	State(state): State<AppState>,
	headers: HeaderMap,
	body: Result<Json<ClientTokenRequest>, JsonRejection>,
) -> Result<Response, HandoffError> {
	let Json(request) = body.map_err(|_| HandoffError::invalid())?;

	// A code string that cannot name a grant is rejected without a lookup.
	if request.code.is_empty() || request.code.len() > client_sessions::MAX_CLIENT_CODE_LEN {
		return Err(HandoffError::invalid());
	}

	// A cross-site browser POST must not burn the code for the real client.
	if !origin_allowed(&state, &headers) {
		return Err(HandoffError::invalid());
	}

	// Burn the code before validating anything else, so a wrong verifier,
	// client, or redirect cannot leave it reusable.
	let Some(grant) = client_sessions::consume_grant(&state.db, &request.code)
		.await
		.map_err(|_| HandoffError::invalid())?
	else {
		return Err(HandoffError::invalid());
	};

	let challenge = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(request.code_verifier.as_bytes()));
	let now = chrono::Utc::now().timestamp();

	if !is_valid_code_verifier(&request.code_verifier)
		|| !redirect_allowed(&state, request.client, &request.redirect_uri)
		|| grant.expires_at <= now
		|| grant.client.as_str() != request.client.as_str()
		|| grant.redirect_uri != request.redirect_uri
		|| !is_valid_code_challenge(&grant.code_challenge)
		|| grant.code_challenge != challenge
	{
		return Err(HandoffError::invalid());
	}

	// The grant's user row exists whenever the grant does: foreign keys
	// cascade user deletion to their grants, so this read cannot miss.
	let (epoch, github_id, github_login, github_avatar_url, plan_tier): (
		i64,
		i64,
		String,
		Option<String>,
		String,
	) = sqlx::query_as(
		"SELECT session_epoch, github_id, github_login, github_avatar_url, plan_tier
		 FROM users WHERE id = ?",
	)
	.bind(grant.user_id)
	.fetch_one(&state.db)
	.await
	.map_err(|_| HandoffError::invalid())?;

	if epoch != grant.session_epoch {
		return Err(HandoffError::invalid());
	}

	let access_token = format!("mcd_{}", uuid::Uuid::new_v4().simple());
	let session = client_sessions::create_session(
		&state.db,
		grant.user_id,
		request.client.as_str(),
		epoch,
		&access_token,
	)
	.await
	.map_err(|_| HandoffError::invalid())?;

	Ok(Json(ClientSession {
		id: session.id,
		access_token,
		expires_at: session.expires_at,
		user: SessionUser {
			github_id,
			github_login,
			github_avatar_url,
			plan_tier,
		},
	})
	.into_response())
}

/// Revoke the caller's own client session. Any well-formed bearer is accepted
/// so a repeated revocation does not reveal whether a token existed.
pub async fn revoke(
	State(state): State<AppState>,
	headers: HeaderMap,
) -> Result<Response, HandoffError> {
	let token = headers
		.get(header::AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.filter(|token| !token.is_empty())
		.ok_or_else(HandoffError::unauthorized)?;

	client_sessions::revoke_session(&state.db, token)
		.await
		.map_err(|_| HandoffError::invalid())?;

	Ok(StatusCode::NO_CONTENT.into_response())
}

/// The browser consent approval input.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproveInput {
	pub client: String,
	pub redirect_uri: String,
	pub code_challenge: String,
	pub state: String,
}

/// Approve a connection and redirect to the client with a one-time code.
pub async fn approve(
	State(state): State<AppState>,
	headers: HeaderMap,
	cookies: CookieJar,
	body: Result<Form<ApproveInput>, FormRejection>,
) -> Result<Response, HandoffError> {
	let Form(input) = body.map_err(|_| HandoffError::invalid())?;

	let origin = state.site_origin().ok_or_else(HandoffError::invalid)?;
	if headers
		.get(header::ORIGIN)
		.and_then(|value| value.to_str().ok())
		!= Some(origin.as_str())
	{
		return Err(HandoffError::forbidden());
	}

	// The one resolver decides: a presented bearer always wins over the
	// cookie, so a hostile Authorization header cannot ride a browser cookie.
	let principal = crate::sessions::resolve_parts(&state, &headers, &cookies)
		.await
		.map_err(|_| HandoffError::invalid())?;
	let Principal::Browser(user) = principal else {
		return Err(HandoffError::unauthorized());
	};

	let client = ClientKind::parse(&input.client).ok_or_else(HandoffError::invalid)?;
	if !redirect_allowed(&state, client, &input.redirect_uri)
		|| !is_valid_code_challenge(&input.code_challenge)
		|| !is_valid_state(&input.state)
	{
		return Err(HandoffError::invalid());
	}

	let code = client_sessions::issue_grant(
		&state.db,
		user.user_id,
		client.as_str(),
		&input.redirect_uri,
		&input.code_challenge,
		user.epoch,
	)
	.await
	.map_err(|_| HandoffError::invalid())?;

	let mut url = url::Url::parse(&input.redirect_uri).map_err(|_| HandoffError::invalid())?;
	url.query_pairs_mut()
		.append_pair("code", &code)
		.append_pair("state", &input.state);

	Ok(Redirect::to(url.as_ref()).into_response())
}

#[cfg(test)]
#[path = "__tests__/client_auth_tests.rs"]
mod tests;
