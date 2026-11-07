use crate::HttpRpcClient;
use crate::HttpTransport;
use crate::MockTransport;
use crate::OrganizationSummary;
use crate::ProjectSummary;
use crate::RpcClient;
use crate::RpcError;
use crate::SessionUser;
use crate::WebsiteRelease;
use crate::WebsiteSection;
use crate::ops;
use crate::project_count;
use crate::release_status_label;

fn sample_session() -> SessionUser {
	SessionUser {
		github_id: 42,
		github_login: "ifiokjr".into(),
		github_avatar_url: Some("https://example.com/a.png".into()),
		plan_tier: "free".into(),
	}
}

fn sample_orgs() -> Vec<OrganizationSummary> {
	vec![
		OrganizationSummary {
			login: "ifiokjr".into(),
			personal: true,
			avatar_url: None,
			supports_projects: true,
			repositories: Vec::new(),
			projects: vec![
				ProjectSummary {
					slug: "monochange".into(),
					name: "monochange".into(),
					description: "Release planning".into(),
					repository_count: 1,
				},
				ProjectSummary {
					slug: "other".into(),
					name: "other".into(),
					description: String::new(),
					repository_count: 0,
				},
			],
		},
		OrganizationSummary {
			login: "acme".into(),
			personal: false,
			avatar_url: None,
			supports_projects: true,
			repositories: Vec::new(),
			projects: vec![ProjectSummary {
				slug: "sdk".into(),
				name: "SDK".into(),
				description: String::new(),
				repository_count: 3,
			}],
		},
	]
}

fn sample_releases() -> Vec<WebsiteRelease> {
	vec![
		WebsiteRelease {
			version: "0.3.3".into(),
			summary: vec!["Feedback SDKs".into()],
			sections: vec![WebsiteSection {
				title: "Added".into(),
				entries: Vec::new(),
			}],
		},
		WebsiteRelease {
			version: "0.3.2".into(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
	]
}

#[tokio::test]
async fn mock_client_reads_session_projects_and_releases() {
	let transport = MockTransport {
		session: Some(sample_session()),
		organizations: sample_orgs(),
		releases: sample_releases(),
		login_url: "https://github.com/login/oauth/authorize?client_id=x".into(),
		fail_unreachable: false,
	};
	let client = HttpRpcClient::new("https://monochange.dev", transport);

	let session = client.session().await.expect("session");
	assert_eq!(session.unwrap().github_login, "ifiokjr");

	let organizations = client.organizations().await.expect("orgs");
	assert_eq!(project_count(&organizations), 3);
	assert_eq!(organizations[0].projects[0].slug, "monochange");

	let releases = client.website_releases().await.expect("releases");
	assert_eq!(release_status_label(&releases), "v0.3.3 on monochange.dev");

	let url = client.login_url(Some("/dashboard")).await.expect("login");
	assert!(url.contains("github.com"));
}

#[tokio::test]
async fn unauthorized_status_maps_to_rpc_error() {
	struct Forbidden;

	impl HttpTransport for Forbidden {
		async fn send(
			&self,
			_method: &str,
			_url: &str,
			_bearer: Option<&str>,
			_body: Option<&[u8]>,
		) -> Result<crate::HttpResponse, RpcError> {
			Ok(crate::HttpResponse {
				status: 401,
				body: Vec::new(),
			})
		}
	}

	let client = HttpRpcClient::new("https://monochange.dev", Forbidden);
	let error = client.session().await.expect_err("unauthorized");
	assert_eq!(error, RpcError::Unauthorized);
}

#[tokio::test]
async fn unreachable_transport_surfaces_cleanly() {
	let transport = MockTransport {
		fail_unreachable: true,
		..MockTransport::default()
	};
	let client = HttpRpcClient::new("http://127.0.0.1:3000", transport);
	let error = client.organizations().await.expect_err("offline");
	assert!(matches!(error, RpcError::Unreachable(_)));
}

#[test]
fn session_and_ops_serde_round_trip() {
	let session = sample_session();
	let encoded = serde_json::to_string(&session).unwrap();
	let decoded: SessionUser = serde_json::from_str(&encoded).unwrap();
	assert_eq!(decoded, session);

	assert_eq!(ops::SESSION, "/rpc/session");
	assert_eq!(ops::ORGANIZATIONS, "/rpc/organizations");
	assert_eq!(ops::WEBSITE_RELEASES, "/rpc/website-releases");
}

#[test]
fn empty_releases_label_is_explicit() {
	assert_eq!(release_status_label(&[]), "No published website releases");
	assert_eq!(project_count(&[]), 0);
}
