//! Project-scoped authentication and origin checks for every mutation.

use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use monochange_app_feedback::FeedbackService;
use monochange_app_feedback::RuleBasedTriage;
use monochange_app_feedback::ServiceError;
use monochange_app_feedback::embedded::ClientSurface;
use monochange_app_feedback::embedded::EmbeddedError;
use monochange_app_feedback::embedded::FeedbackSession;

use super::ApiFailure;
use super::fingerprint;
use super::now;
use crate::AppState;
use crate::feedback::ProjectScope;
use crate::feedback::{self};

pub(super) fn bearer(
	headers: &HeaderMap,
	kind: &str,
	project: i32,
) -> Result<String, EmbeddedError> {
	let token = headers
		.get("authorization")
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.filter(|value| value.len() <= 200)
		.ok_or(EmbeddedError::Unauthorized)?;

	if !token.starts_with(&format!("{kind}.{project}.")) {
		return Err(EmbeddedError::Unauthorized);
	}

	Ok(fingerprint(token))
}

pub(super) async fn scope(state: &AppState, project: i32) -> Result<ProjectScope, ApiFailure> {
	let linked: Option<i64> = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM project_repositories r WHERE r.project_id = p.id) FROM projects p WHERE p.id = $1")
		.bind(project).fetch_optional(&state.db).await.map_err(|_| ApiFailure::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable", "feedback is temporarily unavailable"))?;
	feedback::project_scope(state, project, linked.ok_or(EmbeddedError::Unauthorized)?)
		.await
		.map_err(Into::into)
}

pub(super) fn session(
	service: &FeedbackService<RuleBasedTriage>,
	digest: &str,
	headers: &HeaderMap,
	time: i64,
) -> Result<FeedbackSession, EmbeddedError> {
	let session = service.embedded().session(digest, time)?;
	let integration = service.embedded().integration(&session.integration)?;

	match session.surface {
		ClientSurface::Web => {
			// Same-origin GETs omit Origin. Only the hosted frame may use Fetch Metadata.
			if integration.id == super::iframe::INTEGRATION
				&& !headers.contains_key("origin")
				&& headers
					.get("sec-fetch-site")
					.is_some_and(|value| value == "same-origin")
			{
				return Ok(session);
			}
			let origin = headers
				.get("origin")
				.and_then(|value| value.to_str().ok())
				.ok_or(EmbeddedError::Unauthorized)?;
			if !integration.origins.iter().any(|allowed| allowed == origin) {
				return Err(EmbeddedError::Unauthorized);
			}
		}
		ClientSurface::Mobile => {
			if headers.contains_key("origin") || headers.contains_key("sec-fetch-site") {
				return Err(EmbeddedError::Unauthorized);
			}
		}
	}

	Ok(session)
}

pub(super) async fn mutate<R>(
	state: &AppState,
	scope: &ProjectScope,
	headers: &HeaderMap,
	action: &str,
	maximum: u32,
	mut operation: impl FnMut(
		&mut FeedbackService<RuleBasedTriage>,
		&FeedbackSession,
	) -> Result<R, ServiceError>,
) -> Result<R, ApiFailure> {
	let digest = bearer(headers, "mcfs", scope.project_id)?;
	let time = now();
	feedback::update(state, scope, |service| {
		let session = session(service, &digest, headers, time)?;
		service
			.embedded_mut()
			.quota(&format!("project:{action}"), maximum * 100, 3600, time)?;
		service.embedded_mut().quota(
			&format!("app:{}:{action}", session.integration),
			maximum * 20,
			3600,
			time,
		)?;
		service.embedded_mut().quota(
			&format!("user:{}:{action}", session.subject),
			maximum,
			3600,
			time,
		)?;
		operation(service, &session)
	})
	.await
	.map_err(Into::into)
}

/// Explicit origins only; preflight never grants credentials or bypasses authentication.
pub async fn cors(State(state): State<AppState>, request: Request, next: Next) -> Response {
	let Some(origin) = request.headers().get("origin").cloned() else {
		let mut response = next.run(request).await;
		response
			.headers_mut()
			.insert("cache-control", HeaderValue::from_static("no-store"));

		return response;
	};
	let project = request
		.uri()
		.path()
		.split('/')
		.nth(4)
		.and_then(|value| value.parse().ok());
	let allowed = if let Some(project) = project {
		match scope(&state, project).await {
			Ok(scope) => {
				feedback::load(&state, &scope)
					.await
					.ok()
					.is_some_and(|loaded| {
						loaded.service.embedded().integrations.iter().any(|app| {
							!app.revoked
								&& app.surfaces.contains(&ClientSurface::Web)
								&& app
									.origins
									.iter()
									.any(|allowed| allowed.as_bytes() == origin.as_bytes())
						})
					})
			}
			Err(_) => false,
		}
	} else {
		false
	};

	if !allowed {
		return ApiFailure::new(
			StatusCode::FORBIDDEN,
			"origin_denied",
			"this origin is not registered for feedback",
		)
		.into_response();
	}
	let mut response = if request.method() == Method::OPTIONS {
		StatusCode::NO_CONTENT.into_response()
	} else {
		next.run(request).await
	};

	let headers = response.headers_mut();
	headers.insert("access-control-allow-origin", origin);
	headers.insert(
		"access-control-allow-methods",
		HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
	);
	headers.insert(
		"access-control-allow-headers",
		HeaderValue::from_static("Authorization, Content-Type, Idempotency-Key"),
	);
	headers.insert(
		"access-control-expose-headers",
		HeaderValue::from_static("Retry-After"),
	);

	headers.append("vary", HeaderValue::from_static("Origin"));
	headers.insert("cache-control", HeaderValue::from_static("no-store"));
	response
}
