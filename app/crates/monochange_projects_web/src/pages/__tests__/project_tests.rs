//! A project's settings pick from the organisation's repositories.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use monochange_projects::server_fns::organizations::ProjectRepositoryView;
use monochange_projects::server_fns::organizations::ProjectSummary;
use monochange_projects::server_fns::repos::RepoInfo;

use super::*;

#[tokio::test]
async fn the_edit_form_keeps_the_project_selection() {
	let available = ["acme/api", "acme/web"]
		.into_iter()
		.map(|full_name| {
			RepoInfo {
				id: 1,
				github_full_name: full_name.to_owned(),
				github_private: true,
				plan_tier: "free".to_owned(),
				installation_login: "acme".to_owned(),
				installation_suspended: false,
			}
		})
		.collect();
	let details = ProjectDetails {
		organization: "acme".to_owned(),
		personal: false,
		project: ProjectSummary {
			slug: "invoices".to_owned(),
			name: "Invoices".to_owned(),
			description: String::new(),
			repository_count: 1,
		},
		repositories: vec![ProjectRepositoryView {
			full_name: "Acme/Web".to_owned(),
			private: Some(true),
		}],
		available,
	};
	let html = crate::tests::render_at(
		"/",
		move || view! { <EditProjectForm details=details.clone() /> },
	)
	.await;

	assert!(html.contains("value=\"acme/web\" checked"), "{html}");
	assert!(!html.contains("value=\"acme/api\" checked"), "{html}");
}

#[tokio::test]
async fn a_project_without_repositories_says_where_to_add_them() {
	let details = ProjectDetails {
		organization: "acme".to_owned(),
		personal: false,
		project: ProjectSummary {
			slug: "invoices".to_owned(),
			name: "Invoices".to_owned(),
			description: String::new(),
			repository_count: 0,
		},
		repositories: Vec::new(),
		available: Vec::new(),
	};
	// The page sets its title, so it renders inside a full document.
	let html = crate::tests::render(move || {
		let details = details.clone();
		view! { <leptos_router::components::Router><ProjectContent details=details /></leptos_router::components::Router> }
	})
	.await;

	assert!(
		html.contains("No repositories in this project yet."),
		"{html}"
	);
	assert!(!html.contains("class=\"repository-list\""), "{html}");
	assert!(
		html.contains("button button-quiet\">Edit project"),
		"{html}"
	);
}
