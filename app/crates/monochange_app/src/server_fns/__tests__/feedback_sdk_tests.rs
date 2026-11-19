//! The embedded feedback SDK and worker RPC operations run the REST
//! protocol's handlers behind per-project CORS, through the production
//! router.

#![cfg(not(target_arch = "wasm32"))]
// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::HeaderMap;
use axum::http::Request;
use axum::http::StatusCode;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::embedded_api::admin;
use monochange_app_api::feedback::ProjectScope;
use monochange_app_feedback::embedded::ClientSurface;
use serde_json::Value;
use serde_json::json;
use server_fn::error::FromServerFnError;
use server_fn::error::ServerFnErrorErr;
use tower::ServiceExt;

use super::FeedbackRpcError;
use super::is_sdk_path;
use super::no_store;

const ORIGIN: &str = "https://example.com";

async fn setup() -> (Arc<AppState>, ProjectScope, admin::IssuedCredential) {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	for statement in [
		"INSERT INTO organizations (id, github_id, github_login) VALUES (1, 1, 'acme')",
		"INSERT INTO projects (id, organization_id, slug, name) VALUES (1, 1, 'web', 'Web')",
		"INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'owner', '')",
		"INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type, organization_id) VALUES (1, 1, 1001, 'acme', 'Organization', 1)",
		"INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'acme/web', 0)",
		"INSERT INTO project_repositories (project_id, repository_external_id, full_name) VALUES (1, 11, 'acme/web')",
	] {
		sqlx::query(statement).execute(&db).await.unwrap();
	}
	let secrets: AppSecrets = serde_json::from_value(json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "feedback-sdk-test-signing-key",
		"token_encryption_key": "feedback-sdk-test-token-encryption-key",
		"github_client_id": "feedback-sdk-test-client",
		"github_client_secret": "feedback-sdk-test-client-secret",
	}))
	.unwrap();
	let state = Arc::new(AppState::new(db, secrets).unwrap());
	let scope = monochange_app_api::feedback::public_project(&state, "acme", "web")
		.await
		.unwrap()
		.unwrap()
		.scope;
	let integration = admin::register(
		&state,
		&scope,
		admin::IntegrationInput {
			name: "Web".into(),
			namespace: "accounts".into(),
			origins: vec![ORIGIN.into()],
			surfaces: vec![ClientSurface::Web, ClientSurface::Mobile],
		},
	)
	.await
	.unwrap();
	(state, scope, integration)
}

fn production_router(state: &Arc<AppState>) -> Router {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.build();
	let rate_limit = crate::rate_limit::RateLimit::new(
		crate::rate_limit::RateLimitConfig::default(),
		state.jwt_secret.clone(),
		server_fn::axum::server_fn_paths().map(|(path, _)| path),
	);
	crate::router::build_router(state, options, rate_limit)
}

struct Answer {
	status: StatusCode,
	headers: HeaderMap,
	body: Value,
}

async fn send(app: &Router, request: Request<Body>) -> Answer {
	let response = app.clone().oneshot(request).await.unwrap();
	let status = response.status();
	let headers = response.headers().clone();
	let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
	Answer {
		status,
		headers,
		body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
	}
}

/// POST `arguments` to `operation`, from the browser at [`ORIGIN`] when
/// `browser` is set, with `?project=` in the query.
async fn call(
	app: &Router,
	operation: &str,
	token: &str,
	browser: bool,
	arguments: Value,
) -> Answer {
	let query = if browser { "?project=1" } else { "" };
	let mut request = Request::post(format!("/api/{operation}{query}"))
		.header("authorization", format!("Bearer {token}"))
		.header("content-type", "application/json")
		.header("idempotency-key", "sdk-test-request");
	if browser {
		request = request.header("origin", ORIGIN);
	}
	send(
		app,
		request.body(Body::from(arguments.to_string())).unwrap(),
	)
	.await
}

#[tokio::test]
async fn the_widget_drafts_submits_reads_replies_and_votes_over_rpc() {
	let (state, _, integration) = setup().await;
	let app = production_router(&state);

	let session = call(
		&app,
		"feedback_sdk_session",
		&integration.token,
		false,
		json!({ "project": 1, "input": { "user_id": "user-1", "surface": "web" } }),
	)
	.await;
	assert_eq!(session.status, StatusCode::OK, "{}", session.body);
	assert_eq!(session.headers["cache-control"], "no-store");
	let token = session.body["token"].as_str().unwrap();

	let draft = call(
		&app,
		"feedback_sdk_create_conversation",
		token,
		true,
		json!({ "project": 1, "input": {
			"kind": "feature_request",
			"message": "Export invoices as CSV",
			"context": null,
		} }),
	)
	.await;
	assert_eq!(draft.status, StatusCode::OK, "{}", draft.body);
	assert_eq!(draft.headers["access-control-allow-origin"], ORIGIN);
	let id = draft.body["conversation"]["id"].as_str().unwrap();

	let read = call(
		&app,
		"feedback_sdk_conversation",
		token,
		true,
		json!({ "project": 1, "id": id }),
	)
	.await;
	assert_eq!(read.body["conversation"]["id"], id);

	let updated = call(
		&app,
		"feedback_sdk_conversation_message",
		token,
		true,
		json!({ "project": 1, "id": id, "input": { "body": "Include the total", "version": 1 } }),
	)
	.await;
	assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
	let version = updated.body["conversation"]["version"].clone();

	let receipt = call(
		&app,
		"feedback_sdk_submit",
		token,
		true,
		json!({ "project": 1, "input": { "conversation": id, "version": version } }),
	)
	.await;
	assert_eq!(receipt.status, StatusCode::OK, "{}", receipt.body);
	let item = receipt.body["id"].as_str().unwrap();

	let suggestions = call(
		&app,
		"feedback_sdk_suggestions",
		token,
		true,
		json!({ "project": 1, "input": { "text": "CSV export" } }),
	)
	.await;
	assert_eq!(suggestions.status, StatusCode::OK, "{}", suggestions.body);
	assert!(suggestions.body["reply"].is_string());

	let reply = call(
		&app,
		"feedback_sdk_reply",
		token,
		true,
		json!({ "project": 1, "id": item, "input": { "body": "Also PDF please" } }),
	)
	.await;
	assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);

	for voted in [true, false] {
		let vote = call(
			&app,
			"feedback_sdk_vote",
			token,
			true,
			json!({ "project": 1, "id": item, "voted": voted }),
		)
		.await;
		assert_eq!(vote.status, StatusCode::OK, "{}", vote.body);
		assert_eq!(vote.body["voted"], voted);
	}

	let detail = call(
		&app,
		"feedback_sdk_item",
		token,
		true,
		json!({ "project": 1, "id": item }),
	)
	.await;
	assert_eq!(detail.status, StatusCode::OK, "{}", detail.body);
	assert_eq!(detail.body["voted"], false);
}

#[tokio::test]
async fn workers_pair_claim_heartbeat_and_return_results_over_rpc() {
	let (state, scope, _) = setup().await;
	let app = production_router(&state);
	let item = monochange_app_api::feedback::update(&state, &scope, |service| {
		let receipt = service.receive(monochange_app_feedback::FeedbackSubmission {
			kind: monochange_app_feedback::FeedbackKind::FeatureRequest,
			description: "Export CSV".into(),
			page: None,
			attachments: Vec::new(),
			submitter: monochange_app_feedback::SubmitterIdentity {
				anonymous_id: "reporter".into(),
				email: None,
			},
			app_slug: monochange_app_api::feedback::PORTAL_APP.into(),
		})?;
		Ok(receipt.id)
	})
	.await
	.unwrap();
	let pairing = admin::pairing(&state, &scope, "Local agent", &["acme/web".into()])
		.await
		.unwrap();

	let worker = call(
		&app,
		"feedback_worker_pair",
		&pairing.token,
		false,
		json!({ "project": 1 }),
	)
	.await;
	assert_eq!(worker.status, StatusCode::OK, "{}", worker.body);
	let token = worker.body["token"].as_str().unwrap();

	let job = monochange_app_api::feedback::update(&state, &scope, |service| {
		let commit = "a".repeat(40);
		let version = admin::brief_version(service, &item, "acme/web", &commit)?;
		service
			.planning_mut()
			.queue(
				monochange_app_feedback::planning::PlanningBrief {
					item: item.clone(),
					version,
					repository: "acme/web".into(),
					commit,
					text: "Export CSV".into(),
				},
				false,
				chrono::Utc::now().timestamp(),
			)
			.map_err(Into::into)
	})
	.await
	.unwrap();

	let claimed = call(
		&app,
		"feedback_worker_claim",
		token,
		false,
		json!({ "project": 1 }),
	)
	.await;
	assert_eq!(claimed.body["job"]["id"], job, "{}", claimed.body);
	let lease = claimed.body["lease"].as_str().unwrap();

	let heartbeat = call(
		&app,
		"feedback_worker_heartbeat",
		token,
		false,
		json!({ "project": 1, "id": job, "input": { "lease": lease } }),
	)
	.await;
	assert_eq!(heartbeat.status, StatusCode::OK, "{}", heartbeat.body);

	let result = call(
		&app,
		"feedback_worker_result",
		token,
		false,
		json!({ "project": 1, "id": job, "input": {
			"lease": lease,
			"plan": {
				"summary": "Export invoices",
				"acceptance": ["CSV includes totals"],
				"changes": ["Add export control"],
				"paths": ["src/**"],
				"checks": ["Run export tests"],
				"risks": [],
			},
		} }),
	)
	.await;
	assert_eq!(result.status, StatusCode::OK, "{}", result.body);
}

#[tokio::test]
async fn browsers_need_a_registered_origin_for_the_queried_project() {
	let (state, ..) = setup().await;
	let app = production_router(&state);
	let preflight = |origin: &str, query: &str| {
		Request::options(format!("/api/feedback_sdk_submit{query}"))
			.header("origin", origin)
			.header("access-control-request-method", "POST")
			.body(Body::empty())
			.unwrap()
	};

	let allowed = send(&app, preflight(ORIGIN, "?project=1")).await;
	assert_eq!(allowed.status, StatusCode::NO_CONTENT);
	assert_eq!(allowed.headers["access-control-allow-origin"], ORIGIN);

	let foreign = send(&app, preflight("https://evil.example", "?project=1")).await;
	assert_eq!(foreign.status, StatusCode::FORBIDDEN);
	assert_eq!(foreign.body["code"], "origin_denied");

	let unscoped = send(&app, preflight(ORIGIN, "?draft=1")).await;
	assert_eq!(unscoped.status, StatusCode::FORBIDDEN);

	let mismatched = send(
		&app,
		Request::post("/api/feedback_sdk_item?other=x&project=1")
			.header("origin", ORIGIN)
			.header("authorization", "Bearer mcfs.2.token")
			.header("content-type", "application/json")
			.body(Body::from(
				json!({ "project": 2, "id": "item" }).to_string(),
			))
			.unwrap(),
	)
	.await;
	assert_eq!(mismatched.status, StatusCode::BAD_REQUEST);
	assert_eq!(mismatched.body["code"], "invalid_input");
}

#[tokio::test]
async fn sdk_failures_keep_their_codes() {
	let (state, ..) = setup().await;
	let app = production_router(&state);

	let unauthorized = call(
		&app,
		"feedback_sdk_item",
		"mcfs.1.not-a-session",
		true,
		json!({ "project": 1, "id": "missing" }),
	)
	.await;
	assert_eq!(unauthorized.status, StatusCode::UNAUTHORIZED);
	assert_eq!(unauthorized.body["code"], "unauthorized");
	assert_eq!(unauthorized.headers["cache-control"], "no-store");

	let malformed = call(&app, "feedback_sdk_item", "mcfs.1.x", true, json!({})).await;
	assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
	assert_eq!(malformed.body["code"], "invalid_input");
}

#[tokio::test]
async fn the_hosted_frame_exchanges_the_website_session() {
	let (state, ..) = setup().await;
	let app = production_router(&state);
	let frame = |cookie: Option<String>| {
		let mut request = Request::post("/api/feedback_iframe_session")
			.header("origin", "https://monochange.dev")
			.header("content-type", "application/json");
		if let Some(cookie) = cookie {
			request = request.header("cookie", cookie);
		}
		request
			.body(Body::from(
				json!({ "input": { "organization": "acme", "project": "web" } }).to_string(),
			))
			.unwrap()
	};

	let signed_out = send(&app, frame(None)).await;
	assert_eq!(signed_out.status, StatusCode::UNAUTHORIZED);

	let token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "owner", 0).unwrap();
	let cookie = format!("{}={token}", monochange_app_api::oauth::SESSION_COOKIE_NAME);
	let session = send(&app, frame(Some(cookie))).await;
	assert_eq!(session.status, StatusCode::OK, "{}", session.body);
	assert_eq!(session.body["project"], 1);
	assert!(
		session.body["token"]
			.as_str()
			.unwrap()
			.starts_with("mcfs.1.")
	);
}

#[test]
fn only_cross_origin_sdk_operations_are_gated() {
	assert!(is_sdk_path("/api/feedback_sdk_submit"));
	assert!(is_sdk_path("/api/feedback_worker_result"));
	assert!(!is_sdk_path("/api/feedback_iframe_session"));
	assert!(!is_sdk_path("/api/v1/feedback/1/items"));
}

#[test]
fn rate_limited_answers_say_when_to_retry() {
	let mut headers = HeaderMap::new();
	no_store(&mut headers, StatusCode::TOO_MANY_REQUESTS);
	assert_eq!(headers["retry-after"], "60");
	assert_eq!(headers["cache-control"], "no-store");
}

#[test]
fn framework_failures_carry_codes() {
	let failure = |error| FeedbackRpcError::from_server_fn_error(error);
	let invalid = failure(ServerFnErrorErr::Args("bad".into()));
	assert_eq!(
		(invalid.status, invalid.code.as_str()),
		(400, "invalid_input")
	);
	let broken = failure(ServerFnErrorErr::ServerError("boom".into()));
	assert_eq!((broken.status, broken.code.as_str()), (500, "unavailable"));
	let unknown = FeedbackRpcError {
		status: 1000,
		code: "odd".into(),
		message: "unknown".into(),
	};
	assert_eq!(unknown.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
	assert_eq!(unknown.to_string(), "HTTP 1000 odd: unknown");
}
