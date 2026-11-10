#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;

#[tokio::test]
async fn installation_page_links_to_the_shipped_github_app_chapter() {
	let chapter = crate::book::compiled_book()
		.unwrap()
		.chapter("guide/github-app")
		.unwrap();
	let html = crate::tests::render(|| leptoskit::web::render(install_screen())).await;

	assert!(html.contains(&format!("href=\"{}\"", chapter.href)));
	assert!(html.contains("Read the GitHub App setup guide"));
}
