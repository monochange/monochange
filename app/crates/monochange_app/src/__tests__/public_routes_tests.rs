// The tokio and rstest macros construct their test runtimes with `expect`.
#![allow(clippy::disallowed_methods)]

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::LOCATION;
use rstest::rstest;
use tower::ServiceExt;

use super::book_redirects;

#[rstest]
#[case("/docs", "/book")]
#[case("/docs/", "/book")]
#[case("/docs/index.html", "/book")]
#[case("/docs/readme.md", "/book")]
#[case(
	"/docs/guide/installation.html?from=legacy",
	"/book/guide/installation"
)]
#[tokio::test]
async fn documentation_bookmarks_redirect_without_a_session(
	#[case] path: &str,
	#[case] expected: &str,
) {
	let response = book_redirects()
		.oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
	assert_eq!(response.headers()[LOCATION], expected);
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

#[rstest]
#[case(
	"/book-assets/branding/logo-280.png",
	StatusCode::OK,
	Some("image/png")
)]
#[case("/book-assets/favicon.svg", StatusCode::OK, Some("image/svg+xml"))]
#[case(
	"/book-assets/schemas/monochange.schema.json",
	StatusCode::OK,
	Some("application/json")
)]
#[case("/book-assets/missing.png", StatusCode::NOT_FOUND, None)]
#[case("/book-assets/SUMMARY.md", StatusCode::NOT_FOUND, None)]
#[case("/book-assets/lib.rs", StatusCode::NOT_FOUND, None)]
#[tokio::test]
async fn book_assets_are_limited_to_shipped_public_files(
	#[case] path: &str,
	#[case] status: StatusCode,
	#[case] mime: Option<&str>,
) {
	let response = book_redirects()
		.oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
		.await
		.unwrap();
	assert_eq!(response.status(), status);
	assert_eq!(
		response
			.headers()
			.get(axum::http::header::CONTENT_TYPE)
			.map(|value| value.to_str().unwrap()),
		mime
	);
}
