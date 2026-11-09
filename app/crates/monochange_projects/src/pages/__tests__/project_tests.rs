//! A project's settings pick from the organisation's repositories.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::organizations::ProjectRepositoryView;
use crate::server_fns::organizations::ProjectSummary;
use crate::server_fns::repos::RepoInfo;

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
