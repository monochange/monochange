//! Client session handoff wire contracts.
//!
//! Native guests and the browser extension never receive the website's
//! `__Host-monochange_session` cookie. They open the site in the system
//! browser, approve a connection there, and exchange the resulting one-time
//! code for an opaque client token bound to the same user row. These types are
//! the shared contract for that flow; the browser-side cookie stays in
//! `monochange_app_api::oauth`.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;

use crate::auth::SessionUser;

/// Which non-browser surface is asking for a client session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
	Native,
	Extension,
}

impl ClientKind {
	/// The wire name sent in the browser handoff query and token request.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Native => "native",
			Self::Extension => "extension",
		}
	}

	/// Parse a wire name, rejecting anything the server did not register.
	#[must_use]
	pub fn parse(value: &str) -> Option<Self> {
		match value {
			"native" => Some(Self::Native),
			"extension" => Some(Self::Extension),
			_ => None,
		}
	}
}

impl fmt::Display for ClientKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

/// A one-time code exchange request from a native guest or the extension.
///
/// The code is single-use and short-lived; the verifier proves the caller
/// generated the PKCE challenge that approval bound to the code.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ClientTokenRequest {
	pub client: ClientKind,
	pub code: String,
	pub code_verifier: String,
	pub redirect_uri: String,
}

impl fmt::Debug for ClientTokenRequest {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("ClientTokenRequest")
			.field("client", &self.client)
			.field("code", &"[redacted]")
			.field("code_verifier", &"[redacted]")
			.field("redirect_uri", &self.redirect_uri)
			.finish()
	}
}

/// The opaque credential a client keeps after a successful exchange.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientSession {
	/// Stable identifier the browser can list and revoke.
	pub id: String,
	/// Opaque bearer token. Never a cookie and never a GitHub token.
	pub access_token: String,
	/// Unix seconds when the token stops resolving.
	pub expires_at: i64,
	pub user: SessionUser,
}

impl fmt::Debug for ClientSession {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("ClientSession")
			.field("id", &self.id)
			.field("access_token", &"[redacted]")
			.field("expires_at", &self.expires_at)
			.field("user", &self.user)
			.finish()
	}
}

/// A client session as the browser sees it, without the bearer token.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientSessionInfo {
	pub id: String,
	pub client: ClientKind,
	/// Unix seconds when the session was created.
	pub created_at: i64,
	/// Unix seconds of the last successful use, when one has happened.
	pub last_used_at: Option<i64>,
	/// Unix seconds when the session stops resolving.
	pub expires_at: i64,
}

/// The consent preview returned before the browser approves a connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ConnectPreview {
	/// Registered callback containing only `access_denied` and the validated state.
	pub denial_url: String,
	pub user: SessionUser,
	pub client: ClientKind,
}

/// Generate an RFC 7636 PKCE verifier and its S256 challenge from 32 bytes of
/// randomness. The caller keeps the verifier in memory and sends only the
/// challenge to the site.
///
/// # Panics
/// Panics if the verifier is not 43 characters, which cannot happen for a
/// 32-byte input.
#[must_use]
pub fn pkce_pair(random: [u8; 32]) -> (String, String) {
	let verifier = URL_SAFE_NO_PAD.encode(random);
	let challenge = code_challenge(&verifier);
	(verifier, challenge)
}

/// The S256 challenge for a verifier.
#[must_use]
pub fn code_challenge(verifier: &str) -> String {
	URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()))
}

/// RFC 7636 verifier syntax: 43–128 unreserved characters.
#[must_use]
pub fn is_valid_code_verifier(verifier: &str) -> bool {
	(43..=128).contains(&verifier.len())
		&& verifier
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

/// An S256 challenge is exactly 43 base64url characters.
#[must_use]
pub fn is_valid_code_challenge(challenge: &str) -> bool {
	challenge.len() == 43
		&& challenge
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// The browser handoff state: 16–128 URL-safe characters.
#[must_use]
pub fn is_valid_state(state: &str) -> bool {
	(16..=128).contains(&state.len())
		&& state
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~'))
}

#[cfg(test)]
#[path = "__tests__/client_auth_tests.rs"]
mod tests;

/// In-memory, single-use binding for an interactive client sign-in.
/// Consuming this value prevents a callback being exchanged twice by a client.
pub struct ClientHandoff {
	pub client: ClientKind,
	pub redirect_uri: String,
	pub state: String,
	pub verifier: String,
}
impl ClientHandoff {
	/// Validate the complete callback before releasing its code for exchange.
	pub fn consume(self, callback: &str) -> Result<ClientTokenRequest, &'static str> {
		let mut url = url::Url::parse(callback).map_err(|_| "Invalid callback")?;
		let query: Vec<(String, String)> = url
			.query_pairs()
			.map(|(k, v)| (k.into_owned(), v.into_owned()))
			.collect();
		url.set_query(None);
		if url.as_str() != self.redirect_uri {
			return Err("Unexpected callback address");
		}
		let states: Vec<_> = query.iter().filter(|(key, _)| key == "state").collect();
		if states.len() != 1 || states[0].1 != self.state {
			return Err("Unexpected sign-in state");
		}
		if query.iter().any(|(key, _)| key == "error") {
			return Err("Sign-in was denied or cancelled");
		}
		let codes: Vec<_> = query.iter().filter(|(key, _)| key == "code").collect();
		if codes.len() != 1 || codes[0].1.is_empty() {
			return Err("Missing or ambiguous sign-in code");
		}
		Ok(ClientTokenRequest {
			client: self.client,
			code: codes[0].1.clone(),
			code_verifier: self.verifier,
			redirect_uri: self.redirect_uri,
		})
	}
}
