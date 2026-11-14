//! An organisation's page searches and pages its repositories.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use monochange_projects::server_fns::repos::RepoInfo;

use super::*;

fn organization(count: usize) -> OrganizationSummary {
	with_projects(count, Vec::new(), true)
}

fn with_projects(
	count: usize,
	projects: Vec<monochange_projects::server_fns::organizations::ProjectSummary>,
	supports_projects: bool,
) -> OrganizationSummary {
	OrganizationSummary {
		login: "alice".to_owned(),
		personal: true,
		avatar_url: None,
		supports_projects,
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
		projects,
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
	let html = page("/projects/alice", 30).await;

	assert!(html.contains("30 connected"), "{html}");
	assert!(html.contains("action=\"/projects/alice\""), "{html}");
	assert!(!html.contains("<select"), "{html}");
	assert!(html.contains("Page <!>1<!> of <!>2"), "{html}");
	assert_eq!(html.matches("type=\"checkbox\"").count(), 30, "{html}");
	assert!(html.contains("Filter repositories"), "{html}");
}

#[tokio::test]
async fn searches_count_their_matches() {
	let html = page("/projects/alice?q=repo-01", 30).await;

	assert!(html.contains("10 of 30 match"), "{html}");
	assert!(!html.contains("<nav class=\"pager\""), "{html}");
}

#[tokio::test]
async fn a_single_repository_uses_the_singular_label() {
	let html = page("/projects/alice", 1).await;
	assert!(html.contains("1 repository connected"), "{html}");
}

fn project(slug: &str) -> monochange_projects::server_fns::organizations::ProjectSummary {
	monochange_projects::server_fns::organizations::ProjectSummary {
		slug: slug.to_owned(),
		name: slug.to_uppercase(),
		description: String::new(),
		repository_count: 2,
	}
}

#[tokio::test]
async fn new_project_closes_the_project_grid_as_one_more_card() {
	let html = crate::tests::render_at("/projects/alice", || {
		view! { <OrganizationContent organization=with_projects(2, vec![project("api")], true) /> }
	})
	.await;

	let grid = html.split("<ul class=\"project-grid\">").nth(1).unwrap();
	assert!(
		grid.contains("href=\"/projects/alice/projects/api\""),
		"{html}"
	);
	assert!(grid.contains("<li class=\"project-new\">"), "{html}");
	assert!(!html.contains("No projects yet"), "{html}");

	let empty = page("/projects/alice", 2).await;
	assert!(empty.contains("No projects yet"), "{empty}");
	let empty_grid = empty.split("<ul class=\"project-grid\">").nth(1).unwrap();
	assert!(empty_grid.contains("<li class=\"project-new\">"), "{empty}");
	assert!(!empty_grid.contains("project-card"), "{empty}");
}

#[tokio::test]
async fn unconfirmed_accounts_list_projects_without_the_create_card() {
	let listed = crate::tests::render_at("/projects/alice", || {
		view! { <OrganizationContent organization=with_projects(1, vec![project("api")], false) /> }
	})
	.await;
	assert!(listed.contains("project-grid"), "{listed}");
	assert!(!listed.contains("project-new"), "{listed}");
	assert!(listed.contains("still confirming this account"), "{listed}");

	let empty = crate::tests::render_at("/projects/alice", || {
		view! { <OrganizationContent organization=with_projects(1, Vec::new(), false) /> }
	})
	.await;
	assert!(!empty.contains("project-grid"), "{empty}");
	assert!(empty.contains("still confirming this account"), "{empty}");
}
