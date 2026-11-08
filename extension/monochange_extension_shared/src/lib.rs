//! Shared wire contracts for the monochange browser extension.
//!
//! These types mirror the existing Leptos 0.8 server-function shapes in
//! `monochange_app::server_fns` (`SessionUser`, organisation/project summaries,
//! website releases). When the leptoskit web/RPC port lands a shared crate,
//! replace this module with a path dependency on that crate and delete the
//! stubs — keep the [`RpcClient`] trait and [`HttpRpcClient`] as the extension
//! transport.
//!
//! Endpoints are expressed as leptoskit_rpc-style operation names so a later
//! `leptoskit::rpc::client::call` swap stays mechanical.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

use serde::Deserialize;
use serde::Serialize;

/// Public session payload (`get_session`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUser {
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
}

/// One project under an organisation (`ProjectSummary`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
	pub slug: String,
	pub name: String,
	pub description: String,
	pub repository_count: i64,
}

/// Repository row nested in organisation summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoInfo {
	pub full_name: String,
	pub private: bool,
	#[serde(default)]
	pub installed: bool,
}

/// Connected account with projects (`OrganizationSummary` / `list_organizations`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationSummary {
	pub login: String,
	pub personal: bool,
	pub avatar_url: Option<String>,
	pub supports_projects: bool,
	#[serde(default)]
	pub repositories: Vec<RepoInfo>,
	#[serde(default)]
	pub projects: Vec<ProjectSummary>,
}

/// Website release notes entry (`WebsiteRelease` / `get_website_releases`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsiteRelease {
	pub version: String,
	#[serde(default)]
	pub summary: Vec<String>,
	#[serde(default)]
	pub sections: Vec<WebsiteSection>,
}

/// One section inside a website release document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsiteSection {
	pub title: String,
	#[serde(default)]
	pub entries: Vec<WebsiteEntry>,
}

/// One entry in a website release section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsiteEntry {
	pub summary: String,
	/// Multiline HTML from the server; redacted in snapshots as multiline text.
	#[serde(default)]
	pub details_html: String,
}

/// Failures from the extension's RPC transport.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RpcError {
	#[error("not signed in")]
	Unauthorized,
	#[error("server unreachable: {0}")]
	Unreachable(String),
	#[error("server rejected the request: {0}")]
	Rejected(String),
	#[error("response could not be decoded: {0}")]
	Decode(String),
}

/// Documented RPC operation paths the web port is expected to publish.
///
/// Until leptoskit_rpc is mounted on the app backend, [`HttpRpcClient`] posts
/// JSON to these absolute paths under the configured server origin.
pub mod ops {
	/// `get_session` → optional [`super::SessionUser`].
	pub const SESSION: &str = "/rpc/session";
	/// GitHub OAuth start URL for bearer hosts (`get_login_url`).
	pub const LOGIN_URL: &str = "/rpc/login-url";
	/// End the session (`logout`).
	pub const LOGOUT: &str = "/rpc/logout";
	/// `list_organizations`.
	pub const ORGANIZATIONS: &str = "/rpc/organizations";
	/// `get_website_releases`.
	pub const WEBSITE_RELEASES: &str = "/rpc/website-releases";
}

/// Async RPC surface the popup and background worker share.
///
/// Futures are not `Send`: extension and browser wasm hosts are single-threaded
/// (`JsFuture` is `!Send`).
pub trait RpcClient {
	/// Current session, or `None` when signed out.
	fn session(&self) -> impl std::future::Future<Output = Result<Option<SessionUser>, RpcError>>;

	/// Absolute URL that starts GitHub OAuth for this host.
	fn login_url(
		&self,
		next: Option<&str>,
	) -> impl std::future::Future<Output = Result<String, RpcError>>;

	/// Clears the server session (and the caller should clear local storage).
	fn logout(&self) -> impl std::future::Future<Output = Result<(), RpcError>>;

	/// Organisations and projects the signed-in user can manage.
	fn organizations(
		&self,
	) -> impl std::future::Future<Output = Result<Vec<OrganizationSummary>, RpcError>>;

	/// Published website release status, newest first.
	fn website_releases(
		&self,
	) -> impl std::future::Future<Output = Result<Vec<WebsiteRelease>, RpcError>>;
}

/// How [`HttpRpcClient`] performs one request — inject `reqwest` or a test double.
pub trait HttpTransport {
	fn send(
		&self,
		method: &str,
		url: &str,
		bearer: Option<&str>,
		body: Option<&[u8]>,
	) -> impl std::future::Future<Output = Result<HttpResponse, RpcError>>;
}

/// Minimal HTTP response for the stub client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
	pub status: u16,
	pub body: Vec<u8>,
}

/// JSON-over-HTTP client aimed at the documented `/rpc/*` shapes.
#[derive(Debug, Clone)]
pub struct HttpRpcClient<T> {
	server: String,
	bearer: Option<String>,
	transport: T,
}

impl<T> HttpRpcClient<T> {
	/// Creates a client for `server` (no trailing slash).
	#[must_use]
	pub fn new(server: impl Into<String>, transport: T) -> Self {
		Self {
			server: server.into().trim_end_matches('/').to_owned(),
			bearer: None,
			transport,
		}
	}

	/// Bearer token kept in extension storage (native/extension hosts).
	pub fn set_bearer(&mut self, token: Option<String>) {
		self.bearer = token.filter(|value| !value.is_empty());
	}

	/// The configured API origin.
	#[must_use]
	pub fn server(&self) -> &str {
		&self.server
	}

	fn url(&self, path: &str) -> String {
		format!("{}{path}", self.server)
	}
}

impl<T: HttpTransport> HttpRpcClient<T> {
	async fn call_json<R: for<'de> Deserialize<'de>>(
		&self,
		method: &str,
		path: &str,
		body: Option<&impl Serialize>,
	) -> Result<R, RpcError> {
		let payload = body
			.map(serde_json::to_vec)
			.transpose()
			.map_err(|error| RpcError::Rejected(error.to_string()))?;
		let response = self
			.transport
			.send(
				method,
				&self.url(path),
				self.bearer.as_deref(),
				payload.as_deref(),
			)
			.await?;
		match response.status {
			401 | 403 => return Err(RpcError::Unauthorized),
			status if !(200..300).contains(&status) => {
				let message = String::from_utf8_lossy(&response.body).into_owned();
				return Err(RpcError::Rejected(if message.is_empty() {
					format!("HTTP {status}")
				} else {
					message
				}));
			}
			_ => {}
		}
		if response.body.is_empty() {
			return serde_json::from_str("null")
				.map_err(|error| RpcError::Decode(error.to_string()));
		}
		serde_json::from_slice(&response.body).map_err(|error| RpcError::Decode(error.to_string()))
	}
}

impl<T: HttpTransport> RpcClient for HttpRpcClient<T> {
	async fn session(&self) -> Result<Option<SessionUser>, RpcError> {
		self.call_json("GET", ops::SESSION, None::<&()>).await
	}

	async fn login_url(&self, next: Option<&str>) -> Result<String, RpcError> {
		#[derive(Serialize)]
		struct Body<'a> {
			next: Option<&'a str>,
		}
		#[derive(Deserialize)]
		struct Answer {
			url: String,
		}
		let answer: Answer = self
			.call_json("POST", ops::LOGIN_URL, Some(&Body { next }))
			.await?;
		Ok(answer.url)
	}

	async fn logout(&self) -> Result<(), RpcError> {
		let _: serde_json::Value = self.call_json("POST", ops::LOGOUT, None::<&()>).await?;
		Ok(())
	}

	async fn organizations(&self) -> Result<Vec<OrganizationSummary>, RpcError> {
		self.call_json("GET", ops::ORGANIZATIONS, None::<&()>).await
	}

	async fn website_releases(&self) -> Result<Vec<WebsiteRelease>, RpcError> {
		self.call_json("GET", ops::WEBSITE_RELEASES, None::<&()>)
			.await
	}
}

/// In-memory transport for tests and offline popup demos.
#[derive(Debug, Clone, Default)]
pub struct MockTransport {
	pub session: Option<SessionUser>,
	pub organizations: Vec<OrganizationSummary>,
	pub releases: Vec<WebsiteRelease>,
	pub login_url: String,
	pub fail_unreachable: bool,
}

impl HttpTransport for MockTransport {
	async fn send(
		&self,
		method: &str,
		url: &str,
		_bearer: Option<&str>,
		_body: Option<&[u8]>,
	) -> Result<HttpResponse, RpcError> {
		if self.fail_unreachable {
			return Err(RpcError::Unreachable("mock offline".into()));
		}
		let path = url
			.split_once("://")
			.and_then(|(_, rest)| rest.split_once('/').map(|(_, path)| format!("/{path}")))
			.unwrap_or_else(|| url.to_owned());
		let path = if path.starts_with('/') {
			path
		} else {
			format!("/{path}")
		};
		let (status, body) = match (method, path.as_str()) {
			("GET", ops::SESSION) => (200, serde_json::to_vec(&self.session).unwrap()),
			("POST", ops::LOGIN_URL) => {
				(
					200,
					serde_json::to_vec(&serde_json::json!({ "url": self.login_url })).unwrap(),
				)
			}
			("POST", ops::LOGOUT) => (200, b"null".to_vec()),
			("GET", ops::ORGANIZATIONS) => (200, serde_json::to_vec(&self.organizations).unwrap()),
			("GET", ops::WEBSITE_RELEASES) => (200, serde_json::to_vec(&self.releases).unwrap()),
			_ => (404, format!("unknown {method} {path}").into_bytes()),
		};
		Ok(HttpResponse { status, body })
	}
}

/// Compact status line for the popup header.
#[must_use]
pub fn release_status_label(releases: &[WebsiteRelease]) -> String {
	match releases.first() {
		Some(latest) => format!("v{} on monochange.dev", latest.version),
		None => "No published website releases".to_owned(),
	}
}

/// Counts projects across every organisation for the glance UI.
#[must_use]
pub fn project_count(organizations: &[OrganizationSummary]) -> usize {
	organizations
		.iter()
		.map(|organization| organization.projects.len())
		.sum()
}
