//! The projects error page renders identically on both browser surfaces.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use leptos::prelude::*;
use rstest::rstest;

use crate::error::ErrorTemplate;
use crate::tests::render_at;

#[rstest]
#[case("/projects/acme/missing")]
#[case("/login")]
#[tokio::test]
async fn the_error_page_keeps_the_website_template(#[case] path: &'static str) {
	let html = render_at(path, || {
		view! { <ErrorTemplate status=404 message="Page not found" /> }
	})
	.await;
	assert!(html.contains("status-page"), "{html}");
	assert!(html.contains("404"), "{html}");
	assert!(html.contains("Go home"), "{html}");
	// A projects error page must leave the projects entry as a document
	// navigation when heading home; a website error page stays client-side.
	if monochange_shared::routing::is_projects_path(path) {
		assert!(html.contains("target=\"_self\""), "{html}");
	}
}

#[rstest]
#[case(401, "Not authorized")]
#[case(500, "Server error")]
#[tokio::test]
async fn non_not_found_statuses_render_their_titles(#[case] status: u16, #[case] title: &str) {
	let html = render_at("/p/acme/web", move || {
		view! { <ErrorTemplate status=status message="Something went wrong." /> }
	})
	.await;
	assert!(html.contains(title), "{html}");
}
