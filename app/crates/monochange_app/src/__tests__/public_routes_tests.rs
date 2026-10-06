// The tokio and rstest macros construct their test runtimes with `expect`.
#![allow(clippy::disallowed_methods)]

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::LOCATION;
use rstest::rstest;
use tower::ServiceExt;

use super::book_redirects;
use crate::links::BOOK_URL;

#[rstest]
#[case("/docs")]
#[case("/docs/")]
#[case("/docs/guide/installation?from=legacy")]
#[tokio::test]
async fn documentation_bookmarks_redirect_without_a_session(#[case] path: &str) {
	let response = book_redirects()
		.oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
	assert_eq!(response.headers()[LOCATION], BOOK_URL);
}

#[tokio::test]
async fn other_routes_are_not_redirected_to_documentation() {
	let response = book_redirects()
		.oneshot(
			Request::builder()
				.uri("/pricing")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
