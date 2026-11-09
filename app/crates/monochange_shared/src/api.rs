//! Platform-neutral client for the monochange product HTTP surface.
//!
//! The website's Leptos server functions use `POST` with a form-encoded body
//! and a JSON reply; the handoff endpoints use JSON. This client hides both
//! shapes behind one transport so native guests and the extension do not
//! duplicate wire knowledge.
//!
//! The client owns no credential state: a restored bearer is supplied at
//! construction, `exchange` returns the new session for the caller to
//! persist, and `revoke` acts on the bearer the caller supplies.

use serde::Deserialize;
use thiserror::Error;

use crate::SessionUser;
use crate::client_auth::ClientKind;
use crate::client_auth::ClientSession;
use crate::client_auth::ClientTokenRequest;
use crate::organizations::OrganizationSummary;
use crate::organizations::ProjectDetails;
use crate::releases::WebsiteRelease;

/// Server function endpoint for the signed-in session.
pub const SESSION_PATH: &str = "/api/get_session";
/// Server function endpoint for the user's organizations.
pub const ORGANIZATIONS_PATH: &str = "/api/list_organizations";
/// Server function endpoint for one project.
pub const PROJECT_PATH: &str = "/api/project_overview";
/// Server function endpoint for the website release notes.
pub const WEBSITE_RELEASES_PATH: &str = "/api/get_website_releases";
/// JSON endpoint that exchanges a one-time code for a client session.
pub const TOKEN_PATH: &str = "/api/client-sessions/token";
/// JSON endpoint that revokes the caller's client session.
pub const REVOKE_PATH: &str = "/api/client-sessions/revoke";
/// Browser handoff page the client opens.
pub const CONNECT_PATH: &str = "/connect";

const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
const JSON_CONTENT_TYPE: &str = "application/json";

/// A failure talking to the monochange HTTP surface.
#[derive(Debug, Error)]
pub enum ApiError {
	#[error("transport: {0}")]
	Transport(String),
	#[error("HTTP {status}: {message}")]
	Status { status: u16, message: String },
	#[error("decode: {0}")]
	Decode(String),
}

/// The parts of an HTTP reply the client needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
	pub status: u16,
	pub body: String,
}

impl HttpResponse {
	/// Whether the status is a success.
	#[must_use]
	pub fn is_success(&self) -> bool {
		(200..300).contains(&self.status)
	}
}

/// One `POST` against the monochange HTTP surface.
///
/// The returned future may be `!Send`: browser and extension transports
/// hold `JsValue` handles across `await` on single-threaded wasm hosts, so
/// no `Send` bound is imposed. Callers await the client in place rather
/// than spawning it.
#[allow(async_fn_in_trait)]
pub trait Transport {
	/// Send `body` to `url`, optionally with a bearer token.
	async fn post(
		&self,
		url: &str,
		bearer: Option<&str>,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, ApiError>;
}

/// The shared client used by native guests and the extension.
///
/// The credential is explicit: [`ApiClient::with_bearer`] restores one from
/// the caller's secure store, [`ApiClient::exchange`] returns a new session
/// for the caller to store, and [`ApiClient::revoke`] revokes the bearer the
/// caller passes in. No credential is mutated implicitly, so replacing a
/// stale one is building a new client.
pub struct ApiClient<T> {
	transport: T,
	base_url: url::Url,
	bearer: Option<String>,
}

impl<T: Transport> ApiClient<T> {
	/// Create an anonymous client for the site at `base_url`.
	///
	/// # Panics
	/// Panics when `base_url` is not an absolute URL, which is a configuration
	/// error rather than a runtime failure.
	pub fn new(transport: T, base_url: impl Into<String>) -> Self {
		Self {
			transport,
			base_url: parse_base_url(base_url),
			bearer: None,
		}
	}

	/// Create a client that authenticates every product read with `bearer`,
	/// restored from the caller's secure store after a restart.
	///
	/// # Panics
	/// Panics when `base_url` is not an absolute URL, which is a configuration
	/// error rather than a runtime failure.
	pub fn with_bearer(
		transport: T,
		base_url: impl Into<String>,
		bearer: impl Into<String>,
	) -> Self {
		Self {
			transport,
			base_url: parse_base_url(base_url),
			bearer: Some(bearer.into()),
		}
	}

	/// The bearer this client authenticates with, if any.
	#[must_use]
	pub fn bearer(&self) -> Option<&str> {
		self.bearer.as_deref()
	}

	/// Fetch the signed-in user, or `None` when the credential is not accepted.
	pub async fn session_user(&self) -> Result<Option<SessionUser>, ApiError> {
		let response = self.post(SESSION_PATH, FORM_CONTENT_TYPE, "").await?;
		let value: Option<SessionUser> = decode(response)?;
		Ok(value)
	}

	/// Fetch the organizations the signed-in user can manage.
	pub async fn organizations(&self) -> Result<Vec<OrganizationSummary>, ApiError> {
		let response = self.post(ORGANIZATIONS_PATH, FORM_CONTENT_TYPE, "").await?;
		decode(response)
	}

	/// Fetch one project the signed-in user maintains.
	pub async fn project(
		&self,
		organization: &str,
		project: &str,
	) -> Result<Option<ProjectDetails>, ApiError> {
		let body = url::form_urlencoded::Serializer::new(String::new())
			.append_pair("organization", organization)
			.append_pair("project", project)
			.finish();
		let response = self.post(PROJECT_PATH, FORM_CONTENT_TYPE, &body).await?;
		decode(response)
	}

	/// Fetch the website release notes.
	pub async fn website_releases(&self) -> Result<Vec<WebsiteRelease>, ApiError> {
		let response = self
			.post(WEBSITE_RELEASES_PATH, FORM_CONTENT_TYPE, "")
			.await?;
		decode(response)
	}

	/// The site URL to open in the system browser for the handoff.
	#[must_use]
	pub fn connect_url(
		&self,
		client: ClientKind,
		redirect_uri: &str,
		code_challenge: &str,
		state: &str,
	) -> String {
		let mut url = self.base_url.clone();
		url.set_path(CONNECT_PATH);
		url.query_pairs_mut()
			.append_pair("client", client.as_str())
			.append_pair("redirect_uri", redirect_uri)
			.append_pair("code_challenge", code_challenge)
			.append_pair("code_challenge_method", "S256")
			.append_pair("state", state);
		url.to_string()
	}

	/// Exchange a one-time code for this client's own opaque session.
	///
	/// The returned session is the caller's to store; this client keeps no
	/// credential state. No `Authorization` header is sent, even when this
	/// client was built with a bearer, so a stale stored credential can
	/// never ride the exchange.
	pub async fn exchange(&self, request: ClientTokenRequest) -> Result<ClientSession, ApiError> {
		let body =
			serde_json::to_string(&request).map_err(|error| ApiError::Decode(error.to_string()))?;
		let url = self.join(TOKEN_PATH);
		let response = self
			.transport
			.post(&url, None, JSON_CONTENT_TYPE, &body)
			.await?;
		decode(response)
	}

	/// Revoke the caller-supplied bearer's session. Repeated calls are
	/// harmless server-side; clearing local storage stays with the caller,
	/// including after a failed revocation.
	pub async fn revoke(&self, bearer: &str) -> Result<(), ApiError> {
		let url = self.join(REVOKE_PATH);
		let response = self
			.transport
			.post(&url, Some(bearer), FORM_CONTENT_TYPE, "")
			.await?;
		if !response.is_success() {
			return Err(ApiError::Status {
				status: response.status,
				message: response.body,
			});
		}

		Ok(())
	}

	async fn post(
		&self,
		path: &str,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, ApiError> {
		let url = self.join(path);
		self.transport
			.post(&url, self.bearer.as_deref(), content_type, body)
			.await
	}

	fn join(&self, path: &str) -> String {
		let mut url = self.base_url.clone();
		url.set_path(path);
		url.to_string()
	}
}

fn parse_base_url(base_url: impl Into<String>) -> url::Url {
	url::Url::parse(&base_url.into())
		.unwrap_or_else(|error| panic!("monochange base URL must be absolute: {error}"))
}

fn decode<T: for<'de> Deserialize<'de>>(response: HttpResponse) -> Result<T, ApiError> {
	if !response.is_success() {
		return Err(ApiError::Status {
			status: response.status,
			message: response.body,
		});
	}

	serde_json::from_str(&response.body).map_err(|error| ApiError::Decode(error.to_string()))
}

#[cfg(test)]
#[path = "__tests__/api_tests.rs"]
mod tests;
