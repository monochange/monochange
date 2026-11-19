//! Client sign-in for native guests and the browser extension as RPC.
//!
//! After the browser approves a connection at `/connect/approve`, the native
//! guest or extension exchanges the one-time code for its own opaque session
//! with [`client_session_token`], and signs out with
//! [`client_session_revoke`]. Both are leptoskit RPC operations at
//! `/api/<endpoint>` with JSON arguments and results, beside the product
//! reads the shared `ApiClient` already calls as server functions.
//!
//! [`handoff_gate`] keeps the REST routes' guarantees: every answer is
//! `no-store`, and bodies stop at [`HANDOFF_BODY_LIMIT`]. Failures answer
//! with the REST statuses and a [`HandoffRpcError`] JSON body. The REST
//! routes (`/api/client-sessions/token`, `/api/client-sessions/revoke`) stay
//! for clients released before these operations and run the same cores.

#[cfg(test)]
#[path = "__tests__/client_handoff_tests.rs"]
mod tests;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use leptos::prelude::expect_context;
use leptos::server;
use leptoskit_rpc::api;
use monochange_app_api::AppState;
use monochange_app_api::client_auth;
use monochange_app_api::client_auth::HANDOFF_BODY_LIMIT;
use monochange_app_api::client_auth::HandoffError;
use monochange_shared::ClientSession;
use monochange_shared::ClientTokenRequest;
use serde::Deserialize;
use serde::Serialize;
use server_fn::ServerFn;
use server_fn::codec::Json;
use server_fn::codec::JsonEncoding;
use server_fn::error::FromServerFnError;
use server_fn::error::ServerFnErrorErr;

use crate::server_fns::hosted::framework_status;

/// A failed client sign-in call: its HTTP status and the REST routes'
/// `error` message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRpcError {
	pub status: u16,
	pub error: String,
}

impl std::fmt::Display for HandoffRpcError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "HTTP {}: {}", self.status, self.error)
	}
}

impl From<HandoffError> for HandoffRpcError {
	fn from(error: HandoffError) -> Self {
		Self {
			status: error.status().as_u16(),
			error: error.message().to_string(),
		}
	}
}

impl FromServerFnError for HandoffRpcError {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		let status = framework_status(&value);
		Self {
			status: status.as_u16(),
			error: if status.is_server_error() {
				"handoff unavailable"
			} else {
				"invalid handoff request"
			}
			.to_string(),
		}
	}

	fn status_code(&self) -> StatusCode {
		StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
	}
}

/// Exchange a one-time connection code for this client's opaque session.
///
/// The code is burned before anything else is checked, so a wrong verifier,
/// client, or redirect cannot leave it reusable. A web page's `Origin` is
/// refused without burning it. No `Authorization` header is needed.
///
/// # Arguments
///
/// - `request`: the client kind, one-time code, PKCE verifier, and the
///   redirect URI the code was issued to.
#[api(crate = "leptoskit_rpc", tag = "client-session")]
#[server(endpoint = "client_session_token", input = Json, output = Json)]
pub async fn client_session_token(
	request: ClientTokenRequest,
) -> Result<ClientSession, HandoffRpcError> {
	let state = expect_context::<Arc<AppState>>();
	let parts = expect_context::<Parts>();
	Ok(client_auth::exchange_code(&state, &parts.headers, request).await?)
}

/// Revoke the client session named by the request's bearer. Any
/// well-formed bearer succeeds, so repeating it reveals nothing.
#[api(crate = "leptoskit_rpc", tag = "client-session")]
#[server(endpoint = "client_session_revoke", input = Json, output = Json)]
pub async fn client_session_revoke() -> Result<(), HandoffRpcError> {
	let state = expect_context::<Arc<AppState>>();
	let parts = expect_context::<Parts>();
	Ok(client_auth::revoke_bearer(&state, &parts.headers).await?)
}

/// Whether `path` is one of the client sign-in operations.
#[must_use]
pub fn is_handoff_path(path: &str) -> bool {
	path == ClientSessionToken::PATH || path == ClientSessionRevoke::PATH
}

/// Whether `path` is the revoke operation, the one sign-in call a client
/// session itself may make.
#[must_use]
pub fn is_revoke_path(path: &str) -> bool {
	path == ClientSessionRevoke::PATH
}

/// Limit sign-in bodies to the handoff routes' size and mark every answer,
/// failures included, `no-store`.
pub async fn handoff_gate(request: Request, next: Next) -> Response {
	if !is_handoff_path(request.uri().path()) {
		return next.run(request).await;
	}

	// `DefaultBodyLimit` only stores the limit in the request's extensions,
	// where the server function's body reader finds it.
	let service = tower::ServiceBuilder::new()
		.layer(DefaultBodyLimit::max(HANDOFF_BODY_LIMIT))
		.service(next);
	let mut response = tower::ServiceExt::oneshot(service, request)
		.await
		.unwrap_or_else(|never| match never {});
	response
		.headers_mut()
		.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	response
}
