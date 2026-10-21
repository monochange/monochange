//! Repository lists render the URL's filters, one page at a time.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::repository_filter::REPOSITORIES_PER_PAGE;
use crate::tests::render_at;

fn repository(full_name: &str, private: bool) -> RepoInfo {
	RepoInfo {
		id: 1,
		github_full_name: full_name.to_owned(),
		github_private: private,
		plan_tier: "free".to_owned(),
		installation_login: full_name.split('/').next().unwrap().to_owned(),
		installation_suspended: false,
	}
}

fn numbered(count: usize) -> Vec<RepoInfo> {
	(1..=count)
		.map(|number| repository(&format!("acme/repo-{number:03}"), number % 2 == 0))
		.collect()
}

fn filters(
	uri: &'static str,
	organizations: Vec<String>,
) -> impl std::future::Future<Output = String> {
	render_at(uri, move || {
		let filter = use_repository_filter();
		view! { <RepositoryFilters filter=filter path="/dashboard".to_owned() organizations=organizations.clone() /> }
	})
}

#[tokio::test]
async fn filters_show_the_url_search_and_account() {
	let html = filters(
		"/dashboard?q=web+app&org=Globex",
		vec!["acme".to_owned(), "globex".to_owned()],
	)
	.await;

	assert!(html.contains("method=\"get\""), "{html}");
	assert!(html.contains("action=\"/dashboard\""), "{html}");
	assert!(html.contains("value=\"web app\""), "{html}");
	assert!(html.contains("<option value=\"globex\" selected"), "{html}");
	assert!(!html.contains("<option value=\"\" selected"), "{html}");
	assert!(
		html.contains("<button type=\"submit\" class=\"sr-only\">Search</button>"),
		"{html}"
	);
}

#[tokio::test]
async fn one_account_needs_no_account_filter() {
	let html = filters("/dashboard", vec!["acme".to_owned()]).await;

	assert!(html.contains("Search repositories"), "{html}");
	assert!(!html.contains("<select"), "{html}");
}

#[tokio::test]
async fn every_account_is_selected_without_a_filter() {
	let html = filters("/dashboard", vec!["acme".to_owned(), "globex".to_owned()]).await;

	assert!(html.contains("<option value=\"\" selected"), "{html}");
	assert!(!html.contains("<option value=\"acme\" selected"), "{html}");
}

async fn pager(repositories: Vec<RepoInfo>, search: &'static str) -> String {
	render_at("/", move || {
		view! { <RepositoryPager repositories=repositories.clone() search=Signal::stored(search.to_owned()) label="acme repositories".to_owned() /> }
	})
	.await
}

#[tokio::test]
async fn long_lists_show_the_first_page() {
	let html = pager(numbered(45), "").await;

	assert_eq!(
		html.matches("<li>").count(),
		REPOSITORIES_PER_PAGE,
		"{html}"
	);
	assert!(html.contains("repo-020"), "{html}");
	assert!(!html.contains("repo-021"), "{html}");
	assert!(
		html.contains("class=\"repository-list repository-page\""),
		"{html}"
	);
	assert!(
		html.contains("aria-label=\"acme repositories pages\""),
		"{html}"
	);
	assert!(html.contains("Page <!>1<!> of <!>3"), "{html}");
	assert!(html.contains("Showing <!>1<!>–<!>20<!> of <!>45"), "{html}");
	assert!(html.contains("disabled"), "{html}");
	assert!(html.contains(">Private</span>"), "{html}");
	assert!(html.contains(">Public</span>"), "{html}");
	assert!(html.contains("title=\"acme/repo-001\""), "{html}");
}

#[test]
fn lists_scroll_with_the_page() {
	let stylesheet = include_str!("../../../style/input.css");
	let page_rules = stylesheet
		.lines()
		.filter(|line| line.contains(".repository-page"))
		.collect::<Vec<_>>();

	assert!(!page_rules.is_empty(), "{stylesheet}");
	for rule in page_rules {
		assert!(!rule.contains("max-height"), "{rule}");
		assert!(!rule.contains("overflow"), "{rule}");
	}
	// Containment keeps the wheel from reaching the page at an inner
	// scroller's edge, so no scroller on the site may use it.
	assert!(!stylesheet.contains("overscroll-behavior"), "{stylesheet}");
}

#[tokio::test]
async fn short_lists_have_no_pages() {
	let mut repositories = numbered(3);
	repositories[0].installation_suspended = true;
	let html = pager(repositories, "").await;

	assert_eq!(html.matches("<li>").count(), 3, "{html}");
	assert!(!html.contains("<nav"), "{html}");
	assert!(html.contains("Installation suspended"), "{html}");
}

#[tokio::test]
async fn searches_without_matches_say_so() {
	let html = pager(numbered(3), "  missing ").await;

	assert!(
		html.contains("No repositories match “<!>missing<!>”."),
		"{html}"
	);
	assert!(!html.contains("<ul"), "{html}");
}

async fn picker(repositories: Vec<RepoInfo>, selected: Vec<String>) -> String {
	render_at("/", move || {
		view! { <RepositoryPicker repositories=repositories.clone() selected=selected.clone() /> }
	})
	.await
}

#[tokio::test]
async fn long_pickers_can_be_filtered() {
	let html = picker(
		numbered(PICKER_FILTER_THRESHOLD + 1),
		vec!["acme/repo-002".to_owned()],
	)
	.await;

	assert!(html.contains("Filter repositories"), "{html}");
	assert!(html.contains("form=\"repository-picker-filter\""), "{html}");
	assert_eq!(
		html.matches("type=\"checkbox\"").count(),
		PICKER_FILTER_THRESHOLD + 1
	);
	assert!(html.contains("value=\"acme/repo-002\" checked"), "{html}");
	assert!(
		html.contains("<p hidden class=\"repository-none\""),
		"{html}"
	);
}

#[tokio::test]
async fn short_pickers_skip_the_filter() {
	let html = picker(numbered(2), Vec::new()).await;

	assert!(!html.contains("Filter repositories"), "{html}");
	assert!(!html.contains("checked"), "{html}");
}
