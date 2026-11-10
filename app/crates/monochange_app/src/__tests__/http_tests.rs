use std::panic::AssertUnwindSafe;

use axum::body::Body;
use axum::extract::Request;
use axum::http::Method;

async fn ok(_request: Request<Body>) {}

#[test]
fn every_server_function_method_has_a_route() {
	for method in [
		Method::GET,
		Method::POST,
		Method::PUT,
		Method::DELETE,
		Method::PATCH,
	] {
		let _route = super::method_route(method.clone(), ok);
	}

	let panicked = std::panic::catch_unwind(AssertUnwindSafe(|| {
		let _route = super::method_route(Method::OPTIONS, ok);
	}));
	assert!(panicked.is_err());
}
