//! Browser-bound OAuth state and secure session cookies.

use axum::http::StatusCode;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::cookie::SameSite;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use jsonwebtoken::decode;
use jsonwebtoken::encode;
use serde::Deserialize;
use serde::Serialize;
use time::Duration;

/// Host-only cookie carrying the signed, short-lived OAuth state.
pub const OAUTH_COOKIE_NAME: &str = "__Host-monochange_oauth";
/// Host-only cookie carrying the authenticated session.
pub const SESSION_COOKIE_NAME: &str = "__Host-monochange_session";

#[derive(Serialize, Deserialize)]
struct OAuthState {
	nonce: String,
	exp: usize,
}

/// Start one OAuth attempt, binding its unpredictable state to this browser.
pub fn login_state(secret: &str) -> Result<(String, Cookie<'static>), jsonwebtoken::errors::Error> {
	let nonce = uuid::Uuid::new_v4().to_string();
	let claims = OAuthState {
		nonce: nonce.clone(),
		exp: (chrono::Utc::now() + chrono::Duration::minutes(10)).timestamp() as usize,
	};
	let token = encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret.as_bytes()),
	)?;
	Ok((nonce, oauth_cookie(token, Duration::minutes(10))))
}

/// Reject missing, mismatched, expired, or altered state before exchanging a code.
pub fn verify_login_state(secret: &str, jar: &CookieJar, received: &str) -> Result<(), StatusCode> {
	if received.is_empty() {
		return Err(StatusCode::UNAUTHORIZED);
	}
	let cookie = jar.get(OAUTH_COOKIE_NAME).ok_or(StatusCode::UNAUTHORIZED)?;
	let mut validation = Validation::default();
	validation.leeway = 0;
	let claims = decode::<OAuthState>(
		cookie.value(),
		&DecodingKey::from_secret(secret.as_bytes()),
		&validation,
	)
	.map_err(|_| StatusCode::UNAUTHORIZED)?;
	if claims.claims.nonce != received {
		return Err(StatusCode::UNAUTHORIZED);
	}
	Ok(())
}

/// Consume the browser's pending OAuth attempt, including after failed exchange.
#[must_use]
pub fn clear_login_state() -> Cookie<'static> {
	oauth_cookie(String::new(), Duration::ZERO)
}

fn oauth_cookie(value: String, max_age: Duration) -> Cookie<'static> {
	Cookie::build((OAUTH_COOKIE_NAME, value))
		.path("/")
		.http_only(true)
		.secure(true)
		.same_site(SameSite::Lax)
		.max_age(max_age)
		.build()
}

/// Create a secure session cookie, or expire it when the token is empty.
#[must_use]
pub fn session_cookie(token: String) -> Cookie<'static> {
	let max_age = if token.is_empty() {
		Duration::ZERO
	} else {
		Duration::days(7)
	};
	Cookie::build((SESSION_COOKIE_NAME, token))
		.path("/")
		.http_only(true)
		.secure(true)
		.same_site(SameSite::Lax)
		.max_age(max_age)
		.build()
}

#[cfg(test)]
#[path = "__tests__/oauth_tests.rs"]
mod tests;
