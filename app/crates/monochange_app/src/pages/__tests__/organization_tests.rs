// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use leptos::prelude::*;
use server_fn::ServerFnError;

use super::Unavailable;
use crate::server_fns::organizations::OrganizationSummary;
use crate::server_fns::organizations::ProjectSummary;
use crate::server_fns::repos::RepoInfo;

fn repository(name: &str, private: bool, suspended: bool) -> RepoInfo {
	RepoInfo {
		id: 1,
		github_full_name: name.to_owned(),
		github_private: private,
		plan_tier: "free".to_owned(),
		installation_login: "acme".to_owned(),
		installation_suspended: suspended,
	}
}

fn account(
	personal: bool,
	projects: Vec<ProjectSummary>,
	repositories: Vec<RepoInfo>,
	supports: bool,
) -> OrganizationSummary {
	OrganizationSummary {
		login: "acme".to_owned(),
		personal,
		avatar_url: None,
		supports_projects: supports,
		repositories,
		projects,
	}
}

#[test]
fn the_status_wrapper_renders_the_element_view() {
	let html = Owner::new().with(|| {
		view! { <Unavailable title="Account unavailable" message="Please reload the page to try again." /> }
			.to_html()
	});
	assert!(html.contains("Account unavailable"), "{html}");
}

#[test]
fn action_errors_keep_a_server_message_and_describe_anything_else() {
	assert_eq!(
		super::action_error(&ServerFnError::ServerError("missing account".to_owned())),
		"missing account"
	);
	let other = super::action_error(&ServerFnError::Deserialization("offline".to_owned()));
	assert!(other.contains("offline"), "{other}");
}

#[tokio::test]
async fn an_account_shows_projects_a_repository_filter_and_an_empty_search() {
	let mut repositories = vec![
		repository("acme/private", true, false),
		repository("acme/suspended", false, true),
	];
	for index in 0..21 {
		repositories.push(repository(&format!("acme/repo-{index}"), false, false));
	}
	let organization = account(
		false,
		vec![
			ProjectSummary {
				slug: "invoices".to_owned(),
				name: "Invoices".to_owned(),
				description: String::new(),
				repository_count: 1,
			},
			ProjectSummary {
				slug: "web".to_owned(),
				name: "Web".to_owned(),
				description: "The public site".to_owned(),
				repository_count: 3,
			},
		],
		repositories,
		true,
	);

	let html = crate::tests::render(move || {
		let listed = super::organization_content(&organization, "");
		let personal =
			super::organization_content(&account(true, Vec::new(), Vec::new(), false), "");
		let missing = super::organization_content(&organization, "zzzz-no-such-repo");
		let failure = super::unavailable(
			"Account unavailable",
			"Please reload the page to try again.",
		);
		leptoskit::web::render(crate::ui::with_children(
			crate::ui::column("flex flex-col"),
			[listed, personal, missing, failure],
		))
	})
	.await;

	assert!(html.contains("GitHub organisation"), "{html}");
	assert!(html.contains("Personal account"), "{html}");
	assert!(html.contains("of"), "{html}");
	assert!(html.contains("1 repository"), "{html}");
	assert!(html.contains("3 repositories"), "{html}");
	assert!(html.contains("The public site"), "{html}");
	assert!(html.contains("Filter repositories"), "{html}");
	assert!(html.contains("Private"), "{html}");
	assert!(html.contains("Installation suspended"), "{html}");
	assert!(html.contains("No repositories match"), "{html}");
	assert!(html.contains("Page "), "{html}");
	assert!(html.contains("still confirming this account"), "{html}");
	assert!(html.contains("Account unavailable"), "{html}");
}
