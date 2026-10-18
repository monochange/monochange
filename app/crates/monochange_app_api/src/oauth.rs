//! Browser-bound OAuth state and secure session cookies.

use axum::http::StatusCode;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::cookie::SameSite;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use jsonwebtoken::decode;
use jsonwebtoken::encode;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use time::Duration;

/// Host-only cookie carrying the signed, short-lived OAuth state.
pub const OAUTH_COOKIE_NAME: &str = "__Host-monochange_oauth";
/// Host-only cookie carrying the authenticated session.
pub const SESSION_COOKIE_NAME: &str = "__Host-monochange_session";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthIntent {
	Login,
	Install,
}

#[derive(Serialize, Deserialize)]
struct OAuthState {
	nonce: String,
	intent: OAuthIntent,
	code_verifier: Option<String>,
	/// Where sign-in returns to, already checked by [`return_path`].
	#[serde(default)]
	return_to: Option<String>,
	exp: usize,
}

/// Values needed to begin a browser-bound GitHub authorization.
pub struct PendingOAuth {
	pub state: String,
	pub code_challenge: Option<String>,
	pub cookie: Cookie<'static>,
}

/// Trusted values recovered from the signed browser cookie.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedOAuth {
	pub intent: OAuthIntent,
	pub code_verifier: Option<String>,
	/// The same-site path sign-in started from, if it named one.
	pub return_to: Option<String>,
}

/// The longest path sign-in will return to.
const MAX_RETURN_PATH: usize = 512;

/// `next` when it is a path on this site, so sign-in can return to it
/// without becoming an open redirect. Absolute and protocol-relative URLs,
/// backslashes (which browsers read as slashes), and control characters are
/// refused.
#[must_use]
pub fn return_path(next: &str) -> Option<String> {
	let same_site = next.starts_with('/')
		&& !next.starts_with("//")
		&& next.len() <= MAX_RETURN_PATH
		&& !next.chars().any(|c| c == '\\' || c.is_control());
	same_site.then(|| next.to_owned())
}

/// Start one OAuth attempt, binding its unpredictable state to this browser.
pub fn login_state(secret: &str) -> Result<PendingOAuth, jsonwebtoken::errors::Error> {
	pending_oauth(secret, OAuthIntent::Login, true, None)
}

/// Start a sign-in that returns to `next` afterwards, when [`return_path`]
/// accepts it.
pub fn login_state_returning(
	secret: &str,
	next: &str,
) -> Result<PendingOAuth, jsonwebtoken::errors::Error> {
	pending_oauth(secret, OAuthIntent::Login, true, return_path(next))
}

/// Start a repository installation that GitHub will continue through OAuth.
pub fn installation_state(secret: &str) -> Result<PendingOAuth, jsonwebtoken::errors::Error> {
	pending_oauth(secret, OAuthIntent::Install, false, None)
}

fn pending_oauth(
	secret: &str,
	intent: OAuthIntent,
	with_pkce: bool,
	return_to: Option<String>,
) -> Result<PendingOAuth, jsonwebtoken::errors::Error> {
	let nonce = uuid::Uuid::new_v4().to_string();
	let code_verifier = with_pkce.then(|| {
		format!(
			"{}{}",
			uuid::Uuid::new_v4().simple(),
			uuid::Uuid::new_v4().simple()
		)
	});
	let code_challenge = code_verifier
		.as_ref()
		.map(|verifier| URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes())));
	let claims = OAuthState {
		nonce: nonce.clone(),
		intent,
		code_verifier,
		return_to,
		exp: (chrono::Utc::now() + chrono::Duration::minutes(10)).timestamp() as usize,
	};
	let token = encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret.as_bytes()),
	)?;
	Ok(PendingOAuth {
		state: nonce,
		code_challenge,
		cookie: oauth_cookie(token, Duration::minutes(10)),
	})
}

/// Reject missing, mismatched, expired, or altered state before exchanging a code.
pub fn verify_login_state(
	secret: &str,
	jar: &CookieJar,
	received: &str,
) -> Result<VerifiedOAuth, StatusCode> {
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

	Ok(VerifiedOAuth {
		intent: claims.claims.intent,
		code_verifier: claims.claims.code_verifier,
		return_to: claims.claims.return_to.as_deref().and_then(return_path),
	})
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
