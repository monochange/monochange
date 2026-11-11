//! The projects browser entry renders its own route tree and navigation.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use leptos::prelude::*;

use crate::tests::render_app_at;
use crate::tests::render_at;

#[tokio::test]
async fn the_product_root_renders_the_projects_home() {
	let html = render_app_at("/projects").await;
	assert!(html.contains("Your projects"), "{html}");
	assert!(
		html.contains("aria-label=\"Projects navigation\""),
		"{html}"
	);
	// Cross-entry links in the product navigation must load the website
	// entry as a document navigation.
	assert!(html.contains("target=\"_self\""), "{html}");
}

#[tokio::test]
async fn the_projects_stylesheet_url_points_at_the_projects_bundle() {
	let html = render_app_at("/projects").await;
	assert!(
		html.contains("/pkg/projects/monochange_projects_web.css"),
		"{html}"
	);
	assert!(!html.contains("/pkg/site/monochange_app.css"), "{html}");
}

#[tokio::test]
async fn the_feedback_portal_renders_on_its_public_path() {
	let html = render_app_at("/p/acme/web").await;
	// The portal for an unknown project renders its unavailable state rather
	// than the website's 404 page.
	assert!(
		html.contains("portal") || html.contains("Project not found"),
		"{html}"
	);
}

#[tokio::test]
async fn unknown_product_paths_render_the_projects_error_page() {
	let html = render_app_at("/projects/unknown-organization-xyz").await;
	assert!(html.contains("status-page"), "{html}");
	assert!(
		html.contains("404") || html.contains("not found"),
		"an unknown product path must stay in the projects surface: {html}"
	);
}

#[tokio::test]
async fn the_website_error_template_matches_the_projects_one() {
	let html = render_at("/login", || {
		view! { <monochange_projects::error::ErrorTemplate status=404 message="Page not found" /> }
	})
	.await;
	assert!(html.contains("status-page"), "{html}");
	assert!(html.contains("Go home"), "{html}");
}
