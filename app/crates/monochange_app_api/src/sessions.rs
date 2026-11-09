//! The one authorization resolver every surface funnels through.
//!
//! Browser requests resolve through the host-only session cookie; native and
//! extension requests resolve through their opaque client token. A present
//! `Authorization` header is never allowed to fall back to a valid cookie.

use std::sync::Arc;

use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::cookie::CookieJar;
use monochange_shared::ClientKind;
use monochange_shared::SessionUser;
use server_fn::error::FromServerFnError;
use sqlx::Row;

use crate::AppState;
use crate::oauth::SESSION_COOKIE_NAME;
use crate::verify_token;

/// The signed-in browser user a request resolved to.
#[derive(Debug, Clone)]
pub struct BrowserPrincipal {
	pub user_id: i32,
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
	pub epoch: i64,
}

impl BrowserPrincipal {
	/// The public session user for this browser principal.
	#[must_use]
	pub fn session_user(&self) -> SessionUser {
		SessionUser {
			github_id: self.github_id,
			github_login: self.github_login.clone(),
			github_avatar_url: self.github_avatar_url.clone(),
			plan_tier: self.plan_tier.clone(),
		}
	}
}

/// The native or extension client a request resolved to.
#[derive(Debug, Clone)]
pub struct ClientPrincipal {
	pub session_id: String,
	pub user_id: i32,
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
	pub client: ClientKind,
	pub epoch: i64,
}

/// Who a request is, or why it cannot be anyone.
#[derive(Debug, Clone)]
pub enum Principal {
	/// No credentials were presented.
	Anonymous,
	/// A presented credential failed verification, such as a forged or
	/// expired token. Never falls back to another credential.
	Invalid,
	/// The credential itself verifies, but the account or `session_epoch`
	/// behind it no longer matches: a deleted user, a stale cookie, or a
	/// token from before a global sign-out. Surfaces choose between reading
	/// this as signed out (`get_session`) and rejecting the call.
	Stale,
	Browser(BrowserPrincipal),
	Client(ClientPrincipal),
}

impl Principal {
	/// The user id when the request resolved to a signed-in account.
	#[must_use]
	pub fn user_id(&self) -> Option<i32> {
		match self {
			Self::Browser(user) => Some(user.user_id),
			Self::Client(user) => Some(user.user_id),
			_ => None,
		}
	}

	/// The public session user when the request resolved to an account.
	#[must_use]
	pub fn session_user(&self) -> Option<SessionUser> {
		match self {
			Self::Browser(user) => {
				Some(SessionUser {
					github_id: user.github_id,
					github_login: user.github_login.clone(),
					github_avatar_url: user.github_avatar_url.clone(),
					plan_tier: user.plan_tier.clone(),
				})
			}
			Self::Client(user) => {
				Some(SessionUser {
					github_id: user.github_id,
					github_login: user.github_login.clone(),
					github_avatar_url: user.github_avatar_url.clone(),
					plan_tier: user.plan_tier.clone(),
				})
			}
			_ => None,
		}
	}
}

/// Resolve a request's credentials into a [`Principal`].
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as an
/// anonymous request.
pub async fn resolve_parts(
	state: &AppState,
	headers: &HeaderMap,
	cookies: &CookieJar,
) -> Result<Principal, sqlx::Error> {
	if let Some(auth) = headers.get(AUTHORIZATION) {
		return resolve_client_header(state, auth).await;
	}

	if let Some(cookie) = cookies.get(SESSION_COOKIE_NAME) {
		return resolve_browser(state, cookie.value()).await;
	}

	Ok(Principal::Anonymous)
}

async fn resolve_client_header(
	state: &AppState,
	auth: &axum::http::HeaderValue,
) -> Result<Principal, sqlx::Error> {
	let Ok(value) = auth.to_str() else {
		return Ok(Principal::Invalid);
	};
	let Some(token) = value.strip_prefix("Bearer ") else {
		return Ok(Principal::Invalid);
	};
	if token.is_empty() {
		return Ok(Principal::Invalid);
	}

	let Some(session) =
		monochange_app_db::client_sessions::resolve_session(&state.db, token).await?
	else {
		return Ok(Principal::Invalid);
	};

	let Some(client) = ClientKind::parse(&session.client) else {
		return Ok(Principal::Invalid);
	};

	Ok(Principal::Client(ClientPrincipal {
		session_id: session.id,
		user_id: session.user_id,
		github_id: session.github_id,
		github_login: session.github_login,
		github_avatar_url: session.github_avatar_url,
		plan_tier: session.plan_tier,
		client,
		epoch: session.session_epoch,
	}))
}

/// Resolve a browser session cookie, checking the JWT and the user's current
/// epoch.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as an
/// anonymous request.
pub async fn resolve_browser(state: &AppState, token: &str) -> Result<Principal, sqlx::Error> {
	let Ok(claims) = verify_token(&state.jwt_secret, token) else {
		return Ok(Principal::Invalid);
	};

	let row = sqlx::query(
		"SELECT github_id, github_login, github_avatar_url, plan_tier, session_epoch
		 FROM users WHERE id = ?",
	)
	.bind(claims.sub)
	.fetch_optional(&state.db)
	.await?;

	let Some(row) = row else {
		return Ok(Principal::Stale);
	};

	let github_id: i64 = row.get("github_id");
	let session_epoch: i64 = row.get("session_epoch");
	if github_id != claims.github_id || claims.epoch != session_epoch {
		return Ok(Principal::Stale);
	}

	Ok(Principal::Browser(BrowserPrincipal {
		user_id: claims.sub,
		github_id,
		github_login: row.get("github_login"),
		github_avatar_url: row.get("github_avatar_url"),
		plan_tier: row.get("plan_tier"),
		epoch: session_epoch,
	}))
}

/// Axum middleware that resolves the request once and stores the principal
/// in request extensions for the server functions — SSR pages, API calls,
/// and the handoff alike, so the same credential renders the same identity
/// everywhere. Requests without credentials stay anonymous without touching
/// the database. A resolver failure answers 503 itself: an outage must not
/// masquerade as an anonymous or invalid session downstream.
pub async fn principal_middleware(
	State(state): State<Arc<AppState>>,
	mut request: Request,
	next: Next,
) -> Response {
	let headers = request.headers().clone();
	let cookies = CookieJar::from_headers(&headers);

	match resolve_parts(&state, &headers, &cookies).await {
		Ok(principal) => {
			request.extensions_mut().insert(principal);
			next.run(request).await
		}
		Err(error) => {
			tracing::error!(
				error = %error,
				path = %request.uri().path(),
				"session resolution unavailable"
			);
			unavailable(request.uri().path())
		}
	}
}

/// The 503 a resolution outage answers with. Server-function paths carry
/// the Leptos error encoding so clients decode the failure; everything
/// else answers plain JSON.
fn unavailable(path: &str) -> Response {
	use axum::http::StatusCode;
	use axum::response::IntoResponse;

	const MESSAGE: &str = "Session resolution is temporarily unavailable";

	if server_fn::axum::server_fn_paths().any(|(server_fn_path, _)| server_fn_path == path) {
		let parts = server_fn::ServerFnError::new(MESSAGE).ser();
		let mut response = (
			[(axum::http::header::CONTENT_TYPE, parts.content_type)],
			axum::body::Body::from(parts.body),
		)
			.into_response();
		*response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
		response
	} else {
		(
			StatusCode::SERVICE_UNAVAILABLE,
			axum::Json(serde_json::json!({ "error": MESSAGE })),
		)
			.into_response()
	}
}

#[cfg(test)]
#[path = "__tests__/sessions_tests.rs"]
mod tests;
