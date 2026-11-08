//! Navigation chrome coverage for home vs interior routes.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;

#[test]
fn escape_closes_mobile_navigation() {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	runtime.block_on(async {
		Owner::new().with(|| {
			let (mobile_open, set_mobile_open) = signal(true);
			close_mobile_nav_on_key("Enter", set_mobile_open);
			assert!(mobile_open.get());
			close_mobile_nav_on_key("Escape", set_mobile_open);
			assert!(!mobile_open.get());
		});
	});
}

#[tokio::test]
async fn navbar_on_home_uses_hero_chrome() {
	let html = crate::tests::render_at("/", || view! { <NavBar /> }).await;
	assert!(html.contains("bg-hero/80"));
	assert!(html.contains("Skip to content"));
	assert!(html.contains("aria-label=\"Main navigation\""));
}

#[tokio::test]
async fn navbar_off_home_uses_page_chrome() {
	let html = crate::tests::render_at("/install", || view! { <NavBar /> }).await;
	assert!(html.contains("border-rule bg-page"));
	assert!(html.contains("Install"));
}
