//! Platform-neutral client for the monochange product HTTP surface.
//!
//! The website's Leptos server functions use `POST` with a form-encoded body
//! and a JSON reply; the handoff endpoints use JSON. This client hides both
//! shapes behind one transport so native guests and the extension do not
//! duplicate wire knowledge.

use std::sync::RwLock;

use async_trait::async_trait;
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
#[async_trait]
pub trait Transport: Send + Sync {
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
pub struct ApiClient<T> {
	transport: T,
	base_url: url::Url,
	credential: RwLock<Option<ClientSession>>,
}

impl<T: Transport> ApiClient<T> {
	/// Create a client for the site at `base_url`.
	///
	/// # Panics
	/// Panics when `base_url` is not an absolute URL, which is a configuration
	/// error rather than a runtime failure.
	pub fn new(transport: T, base_url: impl Into<String>) -> Self {
		let base_url = url::Url::parse(&base_url.into())
			.unwrap_or_else(|error| panic!("monochange base URL must be absolute: {error}"));
		Self {
			transport,
			base_url,
			credential: RwLock::new(None),
		}
	}

	/// The stored client session, if one was exchanged.
	#[must_use]
	pub fn session(&self) -> Option<ClientSession> {
		self.credential.read().ok()?.clone()
	}

	/// The stored bearer token, if one was exchanged.
	#[must_use]
	pub fn access_token(&self) -> Option<String> {
		self.credential
			.read()
			.ok()?
			.as_ref()
			.map(|session| session.access_token.clone())
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
	pub async fn exchange(&self, request: ClientTokenRequest) -> Result<ClientSession, ApiError> {
		let body =
			serde_json::to_string(&request).map_err(|error| ApiError::Decode(error.to_string()))?;
		let response = self.post(TOKEN_PATH, JSON_CONTENT_TYPE, &body).await?;
		let session: ClientSession = decode(response)?;
		*self
			.credential
			.write()
			.map_err(|_| ApiError::Transport("credential lock poisoned".into()))? = Some(session.clone());
		Ok(session)
	}

	/// Revoke the stored client session. Repeated calls are harmless.
	pub async fn revoke(&self) -> Result<(), ApiError> {
		let token = self.access_token();
		let Some(token) = token else {
			return Ok(());
		};

		let url = self.join(REVOKE_PATH);
		let response = self
			.transport
			.post(&url, Some(&token), FORM_CONTENT_TYPE, "")
			.await?;
		if !response.is_success() {
			return Err(ApiError::Status {
				status: response.status,
				message: response.body,
			});
		}

		*self
			.credential
			.write()
			.map_err(|_| ApiError::Transport("credential lock poisoned".into()))? = None;
		Ok(())
	}

	async fn post(
		&self,
		path: &str,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, ApiError> {
		let url = self.join(path);
		let bearer = self.access_token();
		self.transport
			.post(&url, bearer.as_deref(), content_type, body)
			.await
	}

	fn join(&self, path: &str) -> String {
		let mut url = self.base_url.clone();
		url.set_path(path);
		url.to_string()
	}
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
