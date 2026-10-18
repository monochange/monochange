#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;

#[test]
fn installation_page_links_to_the_shipped_github_app_chapter() {
	let chapter = crate::book::compiled_book()
		.unwrap()
		.chapter("guide/github-app")
		.unwrap();
	let html = Owner::new().with(|| {
		leptos_meta::provide_meta_context();
		tokio::runtime::Builder::new_current_thread()
			.build()
			.unwrap()
			.block_on(<InstallRoute as leptos_router::LazyRoute>::view(
				InstallRoute::data(),
			))
			.to_html()
	});

	assert!(html.contains(&format!("href=\"{}\"", chapter.href)));
	assert!(html.contains("Read the GitHub App setup guide"));
}
