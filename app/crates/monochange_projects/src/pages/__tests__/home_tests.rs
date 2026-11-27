#![allow(clippy::disallowed_methods)]

use leptos::prelude::*;

use super::WorkspaceEmpty;

#[tokio::test]
async fn organization_avatars_use_the_image_or_an_accessible_initial_fallback() {
	let image = crate::tests::render(|| {
		view! {
			<super::OrganizationAvatar login="acme".to_owned() avatar_url=Some("/avatar.png".to_owned()) />
		}
	})
	.await;
	assert!(image.contains("src=\"/avatar.png\""), "{image}");
	assert!(!image.contains("<span"), "{image}");

	for (login, initial) in [("acme", "A"), ("", "?")] {
		let fallback = crate::tests::render(move || {
			view! {
				<super::OrganizationAvatar login=login.to_owned() />
			}
		})
		.await;
		assert!(fallback.contains("aria-hidden=\"true\""), "{fallback}");
		assert!(
			fallback.contains(&format!(">{initial}</span>")),
			"{fallback}"
		);
		assert!(!fallback.contains("<img"), "{fallback}");
	}
}

#[tokio::test]
async fn empty_workspace_offers_the_next_step_for_each_session() {
	let signed_out = crate::tests::render(|| view! { <WorkspaceEmpty signed_in=false /> }).await;
	assert!(signed_out.contains("Sign in with GitHub"));
	assert!(!signed_out.contains("Connect repositories"));

	let signed_in = crate::tests::render(|| view! { <WorkspaceEmpty signed_in=true /> }).await;
	assert!(signed_in.contains("Connect repositories"));
	assert!(!signed_in.contains("Sign in with GitHub"));
}

fn organization(
	projects: Vec<monochange_shared::ProjectSummary>,
) -> monochange_shared::OrganizationSummary {
	monochange_shared::OrganizationSummary {
		login: "acme".to_owned(),
		personal: false,
		avatar_url: None,
		supports_projects: true,
		repositories: Vec::new(),
		projects,
	}
}

fn project(
	slug: &str,
	description: &str,
	repository_count: i64,
) -> monochange_shared::ProjectSummary {
	monochange_shared::ProjectSummary {
		slug: slug.to_owned(),
		name: slug.to_uppercase(),
		description: description.to_owned(),
		repository_count,
	}
}

#[tokio::test]
async fn project_cards_escape_text_and_link_attributes() {
	let mut summary = project("api", "<script>alert('description')</script>", 2);
	summary.name = "<img src=x onerror=alert('name')>".to_owned();
	let html = crate::tests::render(move || view! {
		<super::ProjectCard href="/projects/test?name=\" onmouseover=\"alert(1)".to_owned() project=summary.clone() />
	}).await;
	assert!(html.contains("&lt;img"), "{html}");
	assert!(html.contains("&lt;script&gt;"), "{html}");
	assert!(!html.contains("<script>"), "{html}");
	assert!(!html.contains("<img src=x"), "{html}");
	assert!(html.contains("&quot; onmouseover=&quot;"), "{html}");
}

#[tokio::test]
async fn each_account_lists_its_projects_as_cards() {
	let projects = vec![project("api", "Billing API", 1), project("web", "", 3)];
	let html = crate::tests::render(
		move || view! { <super::WorkspaceOrganization organization=organization(projects.clone()) /> },
	)
	.await;

	assert!(
		html.contains("<h2><a href=\"/projects/acme\">acme</a></h2>"),
		"{html}"
	);
	assert!(html.contains("2 projects"), "{html}");
	assert!(
		html.contains("href=\"/projects/acme/projects/api\" class=\"project-card\""),
		"{html}"
	);
	assert!(html.contains("<p>Billing API</p>"), "{html}");
	assert!(html.contains("1 repository"), "{html}");
	// A project without a description shows its count once, not twice.
	assert_eq!(html.matches("3 repositories").count(), 1, "{html}");
}

#[tokio::test]
async fn an_account_without_projects_links_to_its_page() {
	let html = crate::tests::render(
		|| view! { <super::WorkspaceOrganization organization=organization(Vec::new()) /> },
	)
	.await;

	assert!(html.contains("0 projects"), "{html}");
	assert!(html.contains("No projects yet."), "{html}");
	assert!(
		html.contains("href=\"/projects/acme\" class=\"text-link\""),
		"{html}"
	);
	assert!(!html.contains("project-grid"), "{html}");

	let single = crate::tests::render(|| view! { <super::WorkspaceOrganization organization=organization(vec![project("api", "", 0)]) /> }).await;
	assert!(single.contains("1 project<"), "{single}");
}

#[tokio::test]
async fn a_workspace_that_fails_to_load_says_how_to_recover() {
	// A forged session cookie fails to resolve, so the workspace load errors.
	let state = crate::tests::feedback::state().await;
	let cookie = format!("{}=forged", monochange_app_api::oauth::SESSION_COOKIE_NAME);
	let html =
		crate::tests::render_with_cookie(state, &cookie, || view! { <super::ProjectsHome /> })
			.await;

	assert!(
		html.contains("<p role=\"alert\" class=\"project-empty\">Projects couldn't be loaded."),
		"{html}"
	);
}
