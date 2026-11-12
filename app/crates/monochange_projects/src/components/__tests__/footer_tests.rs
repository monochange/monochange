//! The shared footer on each browser entry.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;

#[tokio::test]
async fn the_website_footer_links_its_release_and_stays_client_side() {
	let html =
		crate::tests::render_at("/install", || view! { <Footer website_version="1.2.3" /> }).await;

	assert!(html.contains("aria-label=\"Footer navigation\""), "{html}");
	assert!(html.contains("Website v1.2.3"), "{html}");
	assert!(!html.contains("target=\"_self\""), "{html}");
	assert!(
		html.contains("--brand-icon: url('/icons/github.svg')"),
		"{html}"
	);
}

#[tokio::test]
async fn the_projects_footer_reaches_the_website_by_document_navigation() {
	let html = crate::tests::render_at("/projects", || view! { <Footer /> }).await;

	assert!(
		html.contains("href=\"/install\" target=\"_self\""),
		"{html}"
	);
	assert!(html.contains("href=\"/\" target=\"_self\""), "{html}");
	assert!(!html.contains("Website v"), "{html}");
}
