//! An organisation's page searches and pages its repositories.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::repos::RepoInfo;

fn organization(count: usize) -> OrganizationSummary {
	OrganizationSummary {
		login: "alice".to_owned(),
		personal: true,
		avatar_url: None,
		supports_projects: true,
		repositories: (1..=count)
			.map(|number| {
				RepoInfo {
					id: i32::try_from(number).unwrap(),
					github_full_name: format!("alice/repo-{number:03}"),
					github_private: false,
					plan_tier: "free".to_owned(),
					installation_login: "alice".to_owned(),
					installation_suspended: false,
				}
			})
			.collect(),
		projects: Vec::new(),
	}
}

async fn page(uri: &'static str, count: usize) -> String {
	crate::tests::render_at(
		uri,
		move || view! { <OrganizationContent organization=organization(count) /> },
	)
	.await
}

#[tokio::test]
async fn repositories_are_paged_and_projects_pick_from_all_of_them() {
	let html = page("/dashboard/alice", 30).await;

	assert!(html.contains("30 connected"), "{html}");
	assert!(html.contains("action=\"/dashboard/alice\""), "{html}");
	assert!(!html.contains("<select"), "{html}");
	assert!(html.contains("Page <!>1<!> of <!>2"), "{html}");
	assert_eq!(html.matches("type=\"checkbox\"").count(), 30, "{html}");
	assert!(html.contains("Filter repositories"), "{html}");
}

#[tokio::test]
async fn searches_count_their_matches() {
	let html = page("/dashboard/alice?q=repo-01", 30).await;

	assert!(html.contains("10 of 30 match"), "{html}");
	assert!(!html.contains("<nav class=\"pager\""), "{html}");
}
