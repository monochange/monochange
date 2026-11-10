// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

#[tokio::test]
async fn the_organization_route_opens_that_account() {
	let state = crate::tests::memory_state().await;
	let html = crate::tests::render(move || {
		leptos::prelude::provide_context(state.clone());
		leptoskit::web::render(super::page(&super::OrganizationPage {
			organization: "acme".to_owned(),
			q: Some("api".to_owned()),
		}))
	})
	.await;
	assert!(!html.is_empty());
}
