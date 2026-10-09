//! Unit tests for Leptos components and app structure.

use rstest::rstest;

use crate::color_mode::*;
use crate::error::*;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn routes() -> Vec<leptos_axum::AxumRouteListing> {
	// Route discovery temporarily suppresses resource loads process-wide.
	// Match production startup: discover once before any request can run.
	static ROUTES: std::sync::OnceLock<Vec<leptos_axum::AxumRouteListing>> =
		std::sync::OnceLock::new();
	ROUTES
		.get_or_init(|| leptos_axum::generate_route_list(crate::app::App))
		.clone()
}

/// Server-renders `app` inside a minimal document, as a page would be.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn render<V: leptos::prelude::IntoView + 'static>(
	app: impl Fn() -> V + Clone + Send + Sync + 'static,
) -> String {
	use leptos::prelude::*;

	routes();
	let render = leptos_axum::render_app_to_stream_in_order(move || {
		leptos_meta::provide_meta_context();
		view! {
			<!DOCTYPE html>
			<html>
				<head><leptos_meta::MetaTags /></head>
				<body>{app()}</body>
			</html>
		}
	});
	let response = render(axum::http::Request::new(axum::body::Body::empty())).await;
	let body = tokio::time::timeout(
		std::time::Duration::from_secs(10),
		axum::body::to_bytes(response.into_body(), 262_144),
	)
	.await
	.unwrap()
	.unwrap();
	String::from_utf8(body.to_vec()).unwrap()
}

/// Fixtures shared by the feedback console, portal, and page tests.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod feedback {
	use std::sync::Arc;

	use axum::http::Request;
	use axum::http::header::COOKIE;
	use leptos::prelude::*;
	use leptos_axum::ResponseOptions;
	use monochange_app_api::AppSecrets;
	use monochange_app_api::AppState;
	use monochange_app_api::create_token;
	use monochange_app_api::oauth;
	use monochange_app_db::projects::AccountIdentity;
	use monochange_app_db::projects::GITHUB;
	use monochange_app_feedback::FeedbackKind;
	use monochange_app_feedback::FeedbackSubmission;
	use monochange_app_feedback::SubmitterIdentity;

	/// Alice maintains `alice/pocketbook` with a public and a private repository.
	pub(crate) async fn state() -> Arc<AppState> {
		let db = monochange_app_db::create_pool("sqlite::memory:")
			.await
			.unwrap();
		monochange_app_db::run_migrations(&db).await.unwrap();
		sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'unused'), (2, 202, 'bob', 'unused')").execute(&db).await.unwrap();
		sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'alice', 'User'), (2, 2, 1002, 'bob', 'User')").execute(&db).await.unwrap();
		sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'alice/web', 0), (1, 12, 'alice/api', 1), (2, 21, 'bob/app', 0)").execute(&db).await.unwrap();
		let mut connection = db.acquire().await.unwrap();
		let organization = monochange_app_db::projects::link_installation_organization(
			&mut connection,
			1,
			&AccountIdentity {
				provider: GITHUB.to_owned(),
				external_id: 101,
				login: "alice".to_owned(),
				account_type: "User".to_owned(),
				avatar_url: None,
			},
		)
		.await
		.unwrap();
		drop(connection);
		sqlx::query("INSERT INTO projects (id, organization_id, slug, name, description) VALUES (1, $1, 'pocketbook', 'Pocketbook', 'Invoicing')").bind(organization).execute(&db).await.unwrap();
		sqlx::query("INSERT INTO project_repositories (project_id, repository_external_id, full_name) VALUES (1, 11, 'alice/web'), (1, 12, 'alice/api')").execute(&db).await.unwrap();
		let secrets: AppSecrets =
			serde_json::from_value(serde_json::json!({"jwt_secret":"feedback-test-signing-key"}))
				.unwrap();
		Arc::new(AppState::new(db, secrets).unwrap())
	}

	/// A request context signed in as `user` (1 is Alice, 2 is Bob) with an
	/// optional viewer cookie.
	pub(crate) fn context(
		state: &Arc<AppState>,
		user: Option<i32>,
		viewer: Option<&str>,
	) -> (Owner, ResponseOptions) {
		let owner = Owner::new();
		let response = ResponseOptions::default();
		let mut request = Request::new(());
		let mut cookies = Vec::new();
		if let Some(user) = user {
			let (github_id, login) = if user == 1 {
				(101, "alice")
			} else {
				(202, "bob")
			};
			let token = create_token(&state.jwt_secret, user, github_id, login).unwrap();
			cookies.push(format!("{}={token}", oauth::SESSION_COOKIE_NAME));
		}
		if let Some(viewer) = viewer {
			cookies.push(format!(
				"{}={viewer}",
				crate::server_fns::portal::VIEWER_COOKIE
			));
		}
		if !cookies.is_empty() {
			request
				.headers_mut()
				.insert(COOKIE, cookies.join("; ").parse().unwrap());
		}
		let (parts, ()) = request.into_parts();
		owner.with(|| {
			provide_context(state.clone());
			provide_context(parts);
			provide_context(response.clone());
		});
		(owner, response)
	}

	pub(crate) async fn submit(
		state: &Arc<AppState>,
		submitter: &str,
		kind: FeedbackKind,
		description: &str,
	) -> String {
		let public = monochange_app_api::feedback::public_project(state, "alice", "pocketbook")
			.await
			.unwrap()
			.unwrap();
		let submission = FeedbackSubmission {
			kind,
			description: description.to_owned(),
			page: None,
			attachments: Vec::new(),
			submitter: SubmitterIdentity {
				anonymous_id: submitter.to_owned(),
				email: None,
			},
			app_slug: monochange_app_api::feedback::PORTAL_APP.to_owned(),
		};
		monochange_app_api::feedback::update(state, &public.scope, |service| {
			service.receive(submission.clone())
		})
		.await
		.unwrap()
		.id
	}
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
