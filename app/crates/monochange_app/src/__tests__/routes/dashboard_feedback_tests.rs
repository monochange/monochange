// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

#[tokio::test]
async fn the_feedback_route_opens_the_console() {
	let state = crate::tests::memory_state().await;
	let html = crate::tests::render(move || {
		leptos::prelude::provide_context(state.clone());
		leptoskit::web::render(super::page(&super::FeedbackConsolePage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}))
	})
	.await;
	assert!(!html.is_empty());
}
