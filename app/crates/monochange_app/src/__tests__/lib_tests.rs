//! Unit tests for Leptos components and app structure.

use rstest::rstest;

use crate::color_mode::*;
use crate::error::*;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn routes() -> Vec<leptos_axum::AxumRouteListing> {
	// Route discovery temporarily suppresses resource loads process-wide.
	// Share the production listing so this binary discovers exactly once: a
	// second discovery here could suppress a page render running in a
	// parallel test, and that render's resources would never resolve.
	crate::router::routes()
}

/// Server-renders `app` inside a minimal document, as a page would be.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn render<V: leptos::prelude::IntoView + 'static>(
	app: impl Fn() -> V + Clone + Send + Sync + 'static,
) -> String {
	use leptos::prelude::*;

	render_request(
		axum::http::Request::new(axum::body::Body::empty()),
		move || {
			leptos_meta::provide_meta_context();
			view! {
				<!DOCTYPE html>
				<html>
					<head><leptos_meta::MetaTags /></head>
					<body>{app()}</body>
				</html>
			}
		},
	)
	.await
}

/// Server-renders `app` inside a router at `uri`, for components that read
/// the URL.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn render_at<V: leptos::prelude::IntoView + 'static>(
	uri: &'static str,
	app: impl Fn() -> V + Clone + Send + Sync + 'static,
) -> String {
	use leptos::prelude::*;

	let request = axum::http::Request::builder()
		.uri(uri)
		.body(axum::body::Body::empty())
		.unwrap();
	render_request(request, move || {
		let app = app.clone();
		view! { <leptos_router::components::Router>{app()}</leptos_router::components::Router> }
	})
	.await
}

#[cfg(not(target_arch = "wasm32"))]
async fn render_request<V: leptos::prelude::IntoView + 'static>(
	request: axum::http::Request<axum::body::Body>,
	app: impl Fn() -> V + Clone + Send + Sync + 'static,
) -> String {
	routes();
	let render = leptos_axum::render_app_to_stream_in_order(app);
	let response = render(request).await;
	let body = tokio::time::timeout(
		std::time::Duration::from_secs(10),
		axum::body::to_bytes(response.into_body(), 262_144),
	)
	.await
	.unwrap()
	.unwrap();
	String::from_utf8(body.to_vec()).unwrap()
}

// ── AppError tests ──
#[rstest]
#[case(AppError::NotFound("test".into()), 404)]
#[case(AppError::Unauthorized, 401)]
#[case(AppError::Internal("err".into()), 500)]
#[case(AppError::GitHub("api".into()), 502)]
#[case(AppError::Database("db".into()), 500)]
fn test_app_error_status_codes(#[case] error: AppError, #[case] expected: u16) {
	assert_eq!(u16::from(error), expected);
}

#[rstest]
fn test_app_error_display() {
	assert_eq!(
		AppError::NotFound("page".into()).to_string(),
		"Not found: page"
	);
	assert_eq!(
		AppError::Unauthorized.to_string(),
		"Authentication required"
	);
	assert_eq!(
		AppError::Internal("boom".into()).to_string(),
		"Internal server error: boom"
	);
	assert_eq!(
		AppError::GitHub("rate limit".into()).to_string(),
		"GitHub API error: rate limit"
	);
	assert_eq!(
		AppError::Database("timeout".into()).to_string(),
		"Database error: timeout"
	);
}

#[rstest]
fn test_app_error_debug() {
	let err = AppError::NotFound("x".into());
	let debug = format!("{err:?}");
	assert!(debug.contains("NotFound"));
}

// ── ColorMode tests ──
#[rstest]
fn test_color_mode_as_str() {
	assert_eq!(ColorMode::Light.as_str(), "light");
	assert_eq!(ColorMode::Dark.as_str(), "dark");
}

#[rstest]
fn test_color_mode_equality() {
	assert_eq!(ColorMode::Light, ColorMode::Light);
	assert_eq!(ColorMode::Dark, ColorMode::Dark);
	assert_ne!(ColorMode::Light, ColorMode::Dark);
}

#[rstest]
fn test_color_mode_copy() {
	let mode = ColorMode::Light;
	let copied = mode;
	assert_eq!(mode, copied);
}

#[rstest]
fn test_color_mode_clone() {
	let mode = ColorMode::Dark;
	let cloned = mode;
	assert_eq!(mode, cloned);
}

#[rstest]
fn test_color_mode_debug() {
	assert_eq!(format!("{:?}", ColorMode::Light), "Light");
	assert_eq!(format!("{:?}", ColorMode::Dark), "Dark");
}

// ── Color mode state tests (require WASM or browser env) ──
#[cfg(target_arch = "wasm32")]
mod color_mode_state_tests {
	use super::*;

	#[rstest]
	fn test_provide_color_mode_initial_state() {
		let owner = Owner::new();
		owner.set();
		let state = provide_color_mode();
		assert_eq!(state.mode.get(), ColorMode::Light);
	}

	#[rstest]
	fn test_color_mode_toggle() {
		let owner = Owner::new();
		owner.set();
		let state = provide_color_mode();
		assert_eq!(state.mode.get(), ColorMode::Light);
		state.toggle.run(());
		assert_eq!(state.mode.get(), ColorMode::Dark);
		state.toggle.run(());
		assert_eq!(state.mode.get(), ColorMode::Light);
	}

	#[rstest]
	fn test_color_mode_set_mode() {
		let owner = Owner::new();
		owner.set();
		let state = provide_color_mode();
		state.set_mode.run(ColorMode::Dark);
		assert_eq!(state.mode.get(), ColorMode::Dark);
		state.set_mode.run(ColorMode::Light);
		assert_eq!(state.mode.get(), ColorMode::Light);
	}

	#[rstest]
	fn test_use_color_mode_without_provide_panics() {
		let owner = Owner::new();
		owner.set();
		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			use_color_mode();
		}));
		assert!(result.is_err());
	}
}
