//! Embedded feedback and outbound connected-worker protocol.

pub mod admin;
mod assistant;
pub use assistant::AssistantConfig;
mod auth;
mod client;
mod worker;
pub use auth::cors;

#[cfg(test)]
#[path = "__tests__/mod_tests.rs"]
mod tests;

use axum::Json;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use monochange_app_feedback::ServiceError;
use monochange_app_feedback::embedded::EmbeddedError;
use monochange_app_feedback::planning::PlanningError;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::AppState;
use crate::feedback::FeedbackError;

type ApiResult<T> = Result<Json<T>, ApiFailure>;

#[derive(Debug, Serialize)]
pub struct ApiFailure {
	#[serde(skip)]
	pub status: StatusCode,
	pub code: &'static str,
	pub message: String,
}

impl ApiFailure {
	fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
		Self {
			status,
			code,
			message: message.into(),
		}
	}

	fn invalid(message: impl Into<String>) -> Self {
		Self::new(StatusCode::BAD_REQUEST, "invalid_input", message)
	}
}

impl IntoResponse for ApiFailure {
	fn into_response(self) -> Response {
		let status = self.status;
		let mut response = (status, Json(self)).into_response();
		response.headers_mut().insert(
			"cache-control",
			axum::http::HeaderValue::from_static("no-store"),
		);

		if status == StatusCode::TOO_MANY_REQUESTS {
			response
				.headers_mut()
				.insert("retry-after", axum::http::HeaderValue::from_static("60"));
		}

		response
	}
}

impl From<EmbeddedError> for ApiFailure {
	fn from(error: EmbeddedError) -> Self {
		let (status, code) = match error {
			EmbeddedError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
			EmbeddedError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
			EmbeddedError::Conflict => (StatusCode::CONFLICT, "conflict"),
			EmbeddedError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
			EmbeddedError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
		};

		Self::new(status, code, error.to_string())
	}
}

impl From<FeedbackError> for ApiFailure {
	fn from(error: FeedbackError) -> Self {
		match error {
			FeedbackError::Service(ServiceError::Embedded(error)) => error.into(),
			FeedbackError::Service(ServiceError::Planning(error)) => {
				Self::new(
					if error == PlanningError::Budget {
						StatusCode::TOO_MANY_REQUESTS
					} else {
						StatusCode::CONFLICT
					},
					"planning_conflict",
					error.to_string(),
				)
			}

			FeedbackError::Service(error) => Self::invalid(error.to_string()),
			_ => {
				tracing::warn!(%error, "embedded feedback request failed");
				Self::new(
					StatusCode::SERVICE_UNAVAILABLE,
					"unavailable",
					"feedback is temporarily unavailable",
				)
			}
		}
	}
}

#[derive(Debug, Deserialize)]
struct Parameters {
	project: i32,
	id: Option<String>,
}

impl Parameters {
	fn id(&self) -> Result<&str, ApiFailure> {
		self.id
			.as_deref()
			.ok_or_else(|| ApiFailure::invalid("missing resource id"))
	}
}

/// Credentials have distinct roles and a project hint, never shared with login tokens.
pub fn credential(kind: &str, project: i32) -> String {
	format!(
		"{kind}.{project}.{}.{}",
		uuid::Uuid::new_v4().simple(),
		uuid::Uuid::new_v4().simple()
	)
}

pub fn fingerprint(value: &str) -> String {
	hex::encode(Sha256::digest(value.as_bytes()))
}

fn now() -> i64 {
	chrono::Utc::now().timestamp()
}

fn json<T>(
	input: Result<Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<T, ApiFailure> {
	input
		.map(|Json(value)| value)
		.map_err(|_| ApiFailure::invalid("expected a valid JSON request"))
}

/// CORS is checked against project integrations, then against the caller's exact integration.
pub fn router() -> Router<AppState> {
	Router::new().nest(
		"/api/v1/feedback/{project}",
		Router::new()
			.route("/sessions", post(client::session))
			.route("/suggestions", post(client::suggestions))
			.route("/conversations", post(client::create_conversation))
			.route("/conversations/{id}", get(client::show_conversation))
			.route(
				"/conversations/{id}/messages",
				post(client::conversation_message),
			)
			.route("/items", post(client::submit))
			.route("/items/{id}", get(client::item))
			.route("/items/{id}/messages", post(client::reply))
			.route("/items/{id}/vote", put(client::vote).delete(client::unvote))
			.route("/workers/pair", post(worker::pair))
			.route("/workers/claim", post(worker::claim))
			.route("/jobs/{id}/heartbeat", post(worker::heartbeat))
			.route("/jobs/{id}/result", post(worker::result))
			.layer(DefaultBodyLimit::max(1024 * 1024)),
	)
}
