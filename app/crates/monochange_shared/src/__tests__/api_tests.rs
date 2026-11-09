//! The shared API client's transport and encoding behavior.

#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::ApiClient;
use crate::ApiError;
use crate::ClientKind;
use crate::ClientTokenRequest;
use crate::HttpResponse;
use crate::Transport;

const BASE: &str = "https://monochange.test";

#[derive(Clone)]
struct Recorded {
	url: String,
	bearer: Option<String>,
	content_type: String,
	body: String,
}

#[derive(Clone, Default)]
struct MockTransport {
	response: Arc<Mutex<Option<HttpResponse>>>,
	requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockTransport {
	fn new(response: HttpResponse) -> Self {
		Self {
			response: Arc::new(Mutex::new(Some(response))),
			requests: Arc::new(Mutex::new(Vec::new())),
		}
	}

	fn requests(&self) -> Vec<Recorded> {
		self.requests.lock().unwrap().clone()
	}
}

#[async_trait]
impl Transport for MockTransport {
	async fn post(
		&self,
		url: &str,
		bearer: Option<&str>,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, ApiError> {
		self.requests.lock().unwrap().push(Recorded {
			url: url.to_string(),
			bearer: bearer.map(str::to_string),
			content_type: content_type.to_string(),
			body: body.to_string(),
		});
		self.response
			.lock()
			.unwrap()
			.clone()
			.ok_or_else(|| ApiError::Transport("no canned response".into()))
	}
}

fn ok(body: &str) -> HttpResponse {
	HttpResponse {
		status: 200,
		body: body.to_string(),
	}
}

const USER_JSON: &str =
	r#"{"github_id":42,"github_login":"monochange","github_avatar_url":null,"plan_tier":"free"}"#;

fn session_response() -> HttpResponse {
	ok(&format!(
		r#"{{"id":"mcs_1","access_token":"mcd_token","expires_at":1900000000,"user":{USER_JSON}}}"#
	))
}

#[tokio::test]
async fn requests_use_the_shared_wire_shapes() {
	let transport = MockTransport::new(ok("null"));
	let client = ApiClient::new(transport.clone(), BASE);
	let _ = client.session_user().await;
	let _ = client.organizations().await;
	let _ = client.project("acme", "web").await;
	let _ = client.website_releases().await;

	let requests = transport.requests();
	assert_eq!(requests[0].url, format!("{BASE}/api/get_session"));
	assert_eq!(requests[0].bearer, None);
	assert_eq!(
		requests[0].content_type,
		"application/x-www-form-urlencoded"
	);
	assert_eq!(requests[0].body, "");

	assert_eq!(requests[1].url, format!("{BASE}/api/list_organizations"));
	assert_eq!(requests[2].url, format!("{BASE}/api/project_overview"));
	assert_eq!(requests[2].body, "organization=acme&project=web");
	assert_eq!(requests[3].url, format!("{BASE}/api/get_website_releases"));
}

#[tokio::test]
async fn exchange_posts_json_and_stores_the_session() {
	let transport = MockTransport::new(session_response());
	let client = ApiClient::new(transport.clone(), BASE);
	client
		.exchange(ClientTokenRequest {
			client: ClientKind::Native,
			code: "one-time-code".into(),
			code_verifier: "verifier".into(),
			redirect_uri: "dev.monochange.app:/auth/native/return".into(),
		})
		.await
		.unwrap();

	let requests = transport.requests();
	assert_eq!(requests[0].url, format!("{BASE}/api/client-sessions/token"));
	assert_eq!(requests[0].content_type, "application/json");
	assert!(requests[0].body.contains("\"code\":\"one-time-code\""));
	assert!(requests[0].body.contains("\"client\":\"native\""));
	assert!(requests[0].body.contains("\"code_verifier\":\"verifier\""));
	assert_eq!(client.access_token().as_deref(), Some("mcd_token"));
	assert_eq!(client.session().unwrap().user.github_login, "monochange");
}

#[tokio::test]
async fn organizations_use_the_bearer_after_an_exchange() {
	let transport = MockTransport::new(session_response());
	let client = ApiClient::new(transport.clone(), BASE);
	client
		.exchange(ClientTokenRequest {
			client: ClientKind::Native,
			code: "one-time-code".into(),
			code_verifier: "verifier".into(),
			redirect_uri: "dev.monochange.app:/auth/native/return".into(),
		})
		.await
		.unwrap();
	// The exchange consumed the only canned response; give it another.
	*transport.response.lock().unwrap() = Some(ok("[]"));
	let _ = client.organizations().await.unwrap();

	let requests = transport.requests();
	assert_eq!(requests[1].url, format!("{BASE}/api/list_organizations"));
	assert_eq!(requests[1].bearer.as_deref(), Some("mcd_token"));
}

#[tokio::test]
async fn revoke_sends_the_bearer_and_clears_the_credential() {
	let transport = MockTransport::new(session_response());
	let client = ApiClient::new(transport.clone(), BASE);
	client
		.exchange(ClientTokenRequest {
			client: ClientKind::Native,
			code: "one-time-code".into(),
			code_verifier: "verifier".into(),
			redirect_uri: "dev.monochange.app:/auth/native/return".into(),
		})
		.await
		.unwrap();
	*transport.response.lock().unwrap() = Some(ok("null"));
	client.revoke().await.unwrap();
	assert!(client.access_token().is_none());
	assert_eq!(
		transport.requests()[1].url,
		format!("{BASE}/api/client-sessions/revoke")
	);
	assert_eq!(transport.requests()[1].bearer.as_deref(), Some("mcd_token"));
}

#[tokio::test]
async fn session_parses_the_signed_in_user() {
	let transport = MockTransport::new(ok(USER_JSON));
	let client = ApiClient::new(transport, BASE);
	let user = client.session_user().await.unwrap().unwrap();
	assert_eq!(user.github_login, "monochange");
}

#[tokio::test]
async fn project_parses() {
	let transport = MockTransport::new(ok(
		r#"{"organization":"acme","personal":false,"project":{"slug":"web","name":"Web","description":"","repository_count":1},"repositories":[],"available":[]}"#,
	));
	let client = ApiClient::new(transport, BASE);
	let project = client.project("acme", "web").await.unwrap().unwrap();
	assert_eq!(project.project.slug, "web");
}

#[tokio::test]
async fn website_releases_parse() {
	let transport = MockTransport::new(ok(
		r#"[{"version":"1.0.0","summary":["First"],"sections":[]}]"#,
	));
	let client = ApiClient::new(transport, BASE);
	let releases = client.website_releases().await.unwrap();
	assert_eq!(releases[0].version, "1.0.0");
}

#[tokio::test]
async fn revoke_without_a_credential_is_harmless_and_sends_nothing() {
	let transport = MockTransport::new(ok("null"));
	let client = ApiClient::new(transport.clone(), BASE);
	client.revoke().await.unwrap();
	assert!(transport.requests().is_empty());
	assert!(client.access_token().is_none());
}

#[tokio::test]
async fn a_failed_revocation_is_an_error_and_keeps_the_credential() {
	let transport = MockTransport::new(session_response());
	let client = ApiClient::new(transport.clone(), BASE);
	client
		.exchange(ClientTokenRequest {
			client: ClientKind::Native,
			code: "one-time-code".into(),
			code_verifier: "verifier".into(),
			redirect_uri: "dev.monochange.app:/auth/native/return".into(),
		})
		.await
		.unwrap();
	*transport.response.lock().unwrap() = Some(HttpResponse {
		status: 500,
		body: "unavailable".into(),
	});

	let error = client.revoke().await.unwrap_err();
	assert!(matches!(error, ApiError::Status { status: 500, .. }));
	assert_eq!(
		client.access_token().as_deref(),
		Some("mcd_token"),
		"a failed revocation must not clear the local credential"
	);
}

#[test]
fn connect_url_carries_the_handoff_query() {
	let transport = MockTransport::new(ok("null"));
	let client = ApiClient::new(transport, BASE);
	let url = client.connect_url(
		ClientKind::Native,
		"dev.monochange.app:/auth/native/return",
		"challenge",
		"state-123",
	);
	let parsed = url::Url::parse(&url).unwrap();
	assert_eq!(
		parsed.origin().ascii_serialization(),
		"https://monochange.test"
	);
	assert_eq!(parsed.path(), "/connect");
	let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
	assert_eq!(pairs.get("client").map(String::as_str), Some("native"));
	assert_eq!(
		pairs.get("redirect_uri").map(String::as_str),
		Some("dev.monochange.app:/auth/native/return")
	);
	assert_eq!(
		pairs.get("code_challenge").map(String::as_str),
		Some("challenge")
	);
	assert_eq!(
		pairs.get("code_challenge_method").map(String::as_str),
		Some("S256")
	);
	assert_eq!(pairs.get("state").map(String::as_str), Some("state-123"));
}

#[tokio::test]
async fn non_success_status_is_an_error() {
	let transport = MockTransport::new(HttpResponse {
		status: 401,
		body: "unauthorized".into(),
	});
	let client = ApiClient::new(transport, BASE);
	let error = client.session_user().await.unwrap_err();
	assert!(matches!(error, ApiError::Status { status: 401, .. }));
}

#[tokio::test]
async fn undecodable_body_is_an_error() {
	let transport = MockTransport::new(ok("not json"));
	let client = ApiClient::new(transport, BASE);
	let error = client.session_user().await.unwrap_err();
	assert!(matches!(error, ApiError::Decode(_)));
}
