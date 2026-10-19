//! Repository setup remains safe when a previously loaded session expires.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::repos::RepoInfo;

#[tokio::test]
async fn expired_session_offers_sign_in_instead_of_repository_installation() {
	crate::tests::routes();
	let render = leptos_axum::render_app_to_stream_in_order(|| {
		let connection = Resource::new_blocking(
			|| (),
			|()| async { Ok(RepositoryConnectionStatus::SignedOut) },
		);
		view! { <RepositoryConnectionPanel connection=connection /> }
	});
	let response = render(axum::http::Request::new(axum::body::Body::empty())).await;
	let body = tokio::time::timeout(
		std::time::Duration::from_secs(10),
		axum::body::to_bytes(response.into_body(), 16_384),
	)
	.await
	.unwrap()
	.unwrap();
	let html = String::from_utf8(body.to_vec()).unwrap();

	assert!(html.contains("href=\"/login\""), "{html}");
	assert!(html.contains("Sign in to connect repositories"));
	assert!(!html.contains("Connect repositories on GitHub"));
	assert!(!html.contains("https://github.com/apps/"));
}

fn repository(full_name: &str) -> RepoInfo {
	RepoInfo {
		id: 1,
		github_full_name: full_name.to_owned(),
		github_private: false,
		plan_tier: "free".to_owned(),
		installation_login: full_name.split('/').next().unwrap().to_owned(),
		installation_suspended: false,
	}
}

fn organization(login: &str, personal: bool, names: &[&str]) -> OrganizationSummary {
	OrganizationSummary {
		login: login.to_owned(),
		personal,
		avatar_url: None,
		supports_projects: true,
		repositories: names
			.iter()
			.map(|name| repository(&format!("{login}/{name}")))
			.collect(),
		projects: Vec::new(),
	}
}

async fn dashboard(uri: &'static str, organizations: Vec<OrganizationSummary>) -> String {
	crate::tests::render_at(uri, move || {
		let connection = Resource::new_blocking(
			|| (),
			|()| async { Ok(RepositoryConnectionStatus::Unavailable) },
		);
		let user = SessionUser {
			github_id: 101,
			github_login: "alice".to_owned(),
			github_avatar_url: None,
			plan_tier: "free".to_owned(),
		};
		view! { <DashboardContent user=user organizations=organizations.clone() connection=connection /> }
	})
	.await
}

fn workspace() -> Vec<OrganizationSummary> {
	vec![
		organization("alice", true, &["api", "web"]),
		organization("acme", false, &["web-app"]),
	]
}

#[tokio::test]
async fn organisations_start_folded_with_their_counts() {
	let html = dashboard("/dashboard", workspace()).await;

	assert!(html.contains("3 repositories connected"), "{html}");
	assert!(
		html.contains("Personal account · 2 repositories · 0 projects"),
		"{html}"
	);
	assert!(
		html.contains("Organisation · 1 repository · 0 projects"),
		"{html}"
	);
	assert_eq!(html.matches("<details>").count(), 2, "{html}");
	assert!(
		!html.contains("<li hidden class=\"organization-card\""),
		"{html}"
	);
	assert!(html.contains("<select"), "{html}");
	assert!(
		html.contains("<div hidden class=\"repository-empty\""),
		"{html}"
	);
	assert!(
		html.contains("Open <!>acme<!> to manage projects"),
		"{html}"
	);
	assert!(!html.contains("Free</span>"), "{html}");
}

#[tokio::test]
async fn a_single_organisation_starts_open() {
	let html = dashboard("/dashboard", vec![organization("alice", true, &["api"])]).await;

	assert!(html.contains("<details open"), "{html}");
	assert!(!html.contains("<select"), "{html}");
	assert!(html.contains("1 repository connected"), "{html}");
}

#[tokio::test]
async fn searches_open_matching_organisations_and_hide_the_rest() {
	let html = dashboard("/dashboard?q=web", workspace()).await;

	assert!(html.contains("2 of 3 repositories match"), "{html}");
	assert!(
		html.contains("Personal account · 1 of 2 repositories match"),
		"{html}"
	);
	assert_eq!(html.matches("<details open").count(), 2, "{html}");
	assert!(!html.contains(">api</h3>"), "{html}");
}

#[tokio::test]
async fn the_account_filter_hides_other_organisations() {
	let html = dashboard("/dashboard?org=acme", workspace()).await;

	assert!(html.contains("1 of 3 repositories match"), "{html}");
	assert!(
		html.contains("Personal account · 0 of 2 repositories match"),
		"{html}"
	);
	assert_eq!(
		html.matches("<li hidden class=\"organization-card\"")
			.count(),
		1,
		"{html}"
	);
}

#[tokio::test]
async fn searches_without_matches_offer_to_clear_filters() {
	let html = dashboard("/dashboard?q=missing", workspace()).await;

	assert!(html.contains("0 of 3 repositories match"), "{html}");
	assert!(html.contains("Clear filters"), "{html}");
	assert!(
		!html.contains("<div hidden class=\"repository-empty\""),
		"{html}"
	);
}

#[tokio::test]
async fn an_empty_workspace_points_to_setup() {
	let html = dashboard("/dashboard", Vec::new()).await;

	assert!(html.contains("No repositories connected yet"), "{html}");
	assert!(!html.contains("role=\"search\""), "{html}");
}
