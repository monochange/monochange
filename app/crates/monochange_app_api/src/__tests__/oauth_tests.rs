//! OAuth state and cookie security regression tests.

use axum::http::StatusCode;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::cookie::SameSite;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::encode;

use crate::oauth::*;

const SECRET: &str = "oauth-state-test-signing-key";

#[test]
fn valid_state_requires_the_initiating_browser_cookie() {
	let (state, cookie) = login_state(SECRET).unwrap();
	let jar = CookieJar::new().add(cookie);
	assert!(verify_login_state(SECRET, &jar, &state).is_ok());
	assert_eq!(
		verify_login_state(SECRET, &CookieJar::new(), &state),
		Err(StatusCode::UNAUTHORIZED)
	);
	assert_eq!(
		verify_login_state(SECRET, &jar, "another-browser-state"),
		Err(StatusCode::UNAUTHORIZED)
	);
	assert_eq!(
		verify_login_state(SECRET, &jar, ""),
		Err(StatusCode::UNAUTHORIZED)
	);
}

#[test]
fn rejects_expired_tampered_and_wrongly_signed_state() {
	let claims = serde_json::json!({
		"nonce": "expired-state",
		"exp": chrono::Utc::now().timestamp() - 1,
	});
	let expired = encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(SECRET.as_bytes()),
	)
	.unwrap();
	let expired_jar = CookieJar::new().add(Cookie::new(OAUTH_COOKIE_NAME, expired));
	assert_eq!(
		verify_login_state(SECRET, &expired_jar, "expired-state"),
		Err(StatusCode::UNAUTHORIZED)
	);
	let invalid_jar = CookieJar::new().add(Cookie::new(OAUTH_COOKIE_NAME, "tampered"));
	assert_eq!(
		verify_login_state(SECRET, &invalid_jar, "tampered"),
		Err(StatusCode::UNAUTHORIZED)
	);
	let (state, cookie) = login_state(SECRET).unwrap();
	let jar = CookieJar::new().add(cookie);
	assert_eq!(
		verify_login_state("wrong-signing-key", &jar, &state),
		Err(StatusCode::UNAUTHORIZED)
	);
}

#[test]
fn state_is_unique_and_cookies_are_host_bound_and_secure() {
	let (state, cookie) = login_state(SECRET).unwrap();
	let (other, _) = login_state(SECRET).unwrap();
	assert_ne!(state, other);
	assert_eq!(cookie.name(), OAUTH_COOKIE_NAME);
	assert_eq!(cookie.path(), Some("/"));
	assert_eq!(cookie.domain(), None);
	assert_eq!(cookie.http_only(), Some(true));
	assert_eq!(cookie.secure(), Some(true));
	assert_eq!(cookie.same_site(), Some(SameSite::Lax));
	assert_eq!(cookie.max_age(), Some(time::Duration::minutes(10)));
	let cleared = clear_login_state();
	assert_eq!(cleared.name(), OAUTH_COOKIE_NAME);
	assert_eq!(cleared.max_age(), Some(time::Duration::ZERO));
	assert_eq!(cleared.secure(), Some(true));
	assert_eq!(cleared.path(), Some("/"));
}

#[test]
fn session_cookie_and_logout_keep_the_same_security_attributes() {
	let cookie = session_cookie("signed-session".to_string());
	assert_eq!(cookie.name(), SESSION_COOKIE_NAME);
	assert_eq!(cookie.path(), Some("/"));
	assert_eq!(cookie.domain(), None);
	assert_eq!(cookie.http_only(), Some(true));
	assert_eq!(cookie.secure(), Some(true));
	assert_eq!(cookie.same_site(), Some(SameSite::Lax));
	assert_eq!(cookie.max_age(), Some(time::Duration::days(7)));
	let cleared = session_cookie(String::new());
	assert_eq!(cleared.name(), SESSION_COOKIE_NAME);
	assert_eq!(cleared.max_age(), Some(time::Duration::ZERO));
	assert_eq!(cleared.secure(), Some(true));
	assert_eq!(cleared.path(), Some("/"));
}
