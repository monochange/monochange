//! Repository setup remains safe when a previously loaded session expires.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;

#[tokio::test]
async fn expired_session_offers_sign_in_instead_of_repository_installation() {
	crate::tests::routes();
	let render = leptos_axum::render_app_to_stream_in_order(|| {
		let connection = Resource::new_blocking(
			|| (),
			|()| async { Ok(RepositoryConnectionStatus::SignedOut) },
		);
		view! { <RepositoryConnectionPanel connection=connection /> }
	});
	let response = render(axum::http::Request::new(axum::body::Body::empty())).await;
	let body = tokio::time::timeout(
		std::time::Duration::from_secs(10),
		axum::body::to_bytes(response.into_body(), 16_384),
	)
	.await
	.unwrap()
	.unwrap();
	let html = String::from_utf8(body.to_vec()).unwrap();

	assert!(html.contains("href=\"/login\""), "{html}");
	assert!(html.contains("Sign in to connect repositories"));
	assert!(!html.contains("Connect repositories on GitHub"));
	assert!(!html.contains("https://github.com/apps/"));
}
