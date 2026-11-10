// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use leptos::prelude::Owner;

#[test]
fn sign_in_explains_a_missing_or_empty_github_url() {
	let _views = Owner::new().with(|| {
		[
			super::login_message(None),
			super::login_message(Some(String::new())),
			super::login_message(Some("https://github.com/login".to_owned())),
		]
	});
}

#[tokio::test]
async fn an_empty_authorization_code_fails_sign_in() {
	let html = crate::tests::render(|| {
		leptoskit::web::render(super::callback_screen(Some(String::new()), None))
	})
	.await;
	assert!(html.contains("No authorization code"), "{html}");
}
