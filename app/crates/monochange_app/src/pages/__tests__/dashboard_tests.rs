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
		leptoskit::web::render(connection_panel(connection))
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

#[tokio::test]
async fn organization_avatar_renders_the_image_or_the_initial() {
	crate::tests::routes();
	let render = leptos_axum::render_app_to_stream_in_order(|| {
		let login = "acme".to_owned();
		let avatar_url = Some("https://example.com/a.png".to_owned());
		leptos::prelude::view! {
			<OrganizationAvatar login=login.clone() avatar_url=avatar_url />
			<OrganizationAvatar login=login />
		}
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

	assert!(html.contains("https://example.com/a.png"), "{html}");
	assert!(html.contains(">A<"), "{html}");
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
	dashboard_with_updates(uri, organizations, Vec::new()).await
}

async fn dashboard_with_updates(
	uri: &'static str,
	organizations: Vec<OrganizationSummary>,
	updates: Vec<PermissionUpdate>,
) -> String {
	crate::tests::render_at(uri, move || {
		let connection = Resource::new_blocking(
			|| (),
			|()| async { Ok(RepositoryConnectionStatus::Unavailable) },
		);
		let updates = updates.clone();
		let permissions = Resource::new_blocking(
			|| (),
			move |()| {
				let updates = updates.clone();
				async move { Ok(updates) }
			},
		);
		let user = SessionUser {
			github_id: 101,
			github_login: "alice".to_owned(),
			github_avatar_url: None,
			plan_tier: "free".to_owned(),
		};
		let filter = crate::repository_filter::RepositoryFilter::new(
			uri.split_once("q=")
				.map(|(_, value)| value.split('&').next().unwrap_or(value).replace('+', " ")),
			uri.split_once("org=")
				.map(|(_, value)| value.split('&').next().unwrap_or(value).to_owned()),
		);
		leptoskit::web::render(workspace(
			&user,
			&organizations,
			connection,
			permissions,
			&filter,
		))
	})
	.await
}

fn sample_workspace() -> Vec<OrganizationSummary> {
	vec![
		organization("alice", true, &["api", "web"]),
		organization("acme", false, &["web-app"]),
	]
}

#[tokio::test]
async fn organisations_start_folded_with_their_counts() {
	let html = dashboard("/dashboard", sample_workspace()).await;

	assert!(html.contains("3 repositories connected"), "{html}");
	assert!(
		html.contains("Personal account · 2 repositories · 0 projects"),
		"{html}"
	);
	assert!(
		html.contains("Organisation · 1 repository · 0 projects"),
		"{html}"
	);
	assert!(
		html.contains("role=\"search\"") || html.contains("Filter repositories"),
		"{html}"
	);
	assert!(html.contains("Open acme to manage projects"), "{html}");
	assert!(!html.contains("Free</span>"), "{html}");
}

#[tokio::test]
async fn installations_waiting_for_new_permissions_link_to_github() {
	let html = dashboard_with_updates(
		"/dashboard",
		sample_workspace(),
		vec![PermissionUpdate {
			account: "acme".to_owned(),
			review_url: "https://github.com/organizations/acme/settings/installations/22"
				.to_owned(),
			missing: "Actions: write (now none), Pull requests: write (now read)".to_owned(),
		}],
	)
	.await;

	assert!(
		html.contains("Approve monochange&#x27;s new GitHub permissions")
			|| html.contains("Approve monochange's new GitHub permissions"),
		"{html}"
	);
	assert!(
		html.contains("Actions: write (now none), Pull requests: write (now read)"),
		"{html}"
	);
	assert!(
		html.contains("href=\"https://github.com/organizations/acme/settings/installations/22\""),
		"{html}"
	);
	assert!(html.contains("Review on GitHub"), "{html}");
}

#[tokio::test]
async fn no_permission_notice_when_every_installation_is_current() {
	let html = dashboard("/dashboard", sample_workspace()).await;

	assert!(!html.contains("permission-update"), "{html}");
}

#[tokio::test]
async fn a_single_organisation_starts_open() {
	let html = dashboard("/dashboard", vec![organization("alice", true, &["api"])]).await;

	assert!(html.contains("alice/api") || html.contains("api"), "{html}");
	assert!(!html.contains("<select"), "{html}");
	assert!(html.contains("1 repository connected"), "{html}");
}

#[tokio::test]
async fn searches_open_matching_organisations_and_hide_the_rest() {
	let html = dashboard("/dashboard?q=web", sample_workspace()).await;

	assert!(html.contains("2 of 3 repositories match"), "{html}");
	assert!(
		html.contains("Personal account · 1 of 2 repositories match"),
		"{html}"
	);
	assert!(!html.contains("alice/api"), "{html}");
}

#[tokio::test]
async fn the_account_filter_hides_other_organisations() {
	let html = dashboard("/dashboard?org=acme", sample_workspace()).await;

	assert!(html.contains("1 of 3 repositories match"), "{html}");
	assert!(
		html.contains("Personal account · 0 of 2 repositories match"),
		"{html}"
	);
	assert!(!html.contains("alice/api"), "{html}");
	assert!(
		html.contains("acme/web-app") || html.contains("web-app"),
		"{html}"
	);
}

#[tokio::test]
async fn searches_without_matches_offer_to_clear_filters() {
	let html = dashboard("/dashboard?q=missing", sample_workspace()).await;

	assert!(html.contains("0 of 3 repositories match"), "{html}");
	assert!(html.contains("Clear filters"), "{html}");
	assert!(html.contains("No repositories match"), "{html}");
}

#[tokio::test]
async fn an_empty_workspace_points_to_setup() {
	let html = dashboard("/dashboard", Vec::new()).await;

	assert!(html.contains("No repositories connected yet"), "{html}");
	assert!(!html.contains("role=\"search\""), "{html}");
}

#[tokio::test]
async fn current_installations_and_an_available_app_render_their_states() {
	let html = crate::tests::render(|| {
		let permissions = Resource::new_blocking(|| (), |()| async { Ok(Vec::new()) });
		let connection = Resource::new_blocking(
			|| (),
			|()| {
				async {
					Ok(RepositoryConnectionStatus::Available {
						installation_url: "https://github.com/apps/monochange/installations/new"
							.to_owned(),
					})
				}
			},
		);
		leptoskit::web::render(crate::ui::with_children(
			crate::ui::column("flex flex-col"),
			[permission_notice(permissions), connection_panel(connection)],
		))
	})
	.await;

	assert!(!html.contains("Approve monochange"), "{html}");
	assert!(html.contains("Connect your repositories"), "{html}");
	assert!(html.contains("Connect repositories on GitHub"), "{html}");
}
