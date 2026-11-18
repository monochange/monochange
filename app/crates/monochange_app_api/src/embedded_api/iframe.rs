//! First-party account exchange for the hosted, HTML-only feedback frame.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::response::Response;
use axum_extra::extract::cookie::CookieJar;
use monochange_app_feedback::embedded::ClientSurface;
use monochange_app_feedback::embedded::EmbeddedError;
use monochange_app_feedback::embedded::FeedbackSession;
use monochange_app_feedback::embedded::Integration;
use serde::Deserialize;
use serde_json::json;

use super::ApiFailure;
use super::credential;
use super::fingerprint;
use super::now;
use crate::AppState;
use crate::feedback;

pub(super) const INTEGRATION: &str = "monochange-iframe";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionInput {
	organization: String,
	project: String,
}

/// Only the first-party popup exchanges login cookies; framed API calls use bearer tokens.
pub(super) async fn session(
	State(state): State<AppState>,
	headers: HeaderMap,
	cookies: CookieJar,
	input: Result<Json<SessionInput>, JsonRejection>,
) -> Result<Response, ApiFailure> {
	let input = super::json(input)?;
	let origin = url::Url::parse(&state.github_callback_url)
		.map_err(|_| ApiFailure::invalid("invalid sign-in callback URL"))?
		.origin()
		.ascii_serialization();
	let allowed = super::admin::origins(std::slice::from_ref(&origin))?;

	if headers.get("origin").and_then(|value| value.to_str().ok()) != Some(origin.as_str()) {
		return Err(EmbeddedError::Unauthorized.into());
	}

	let login = cookies
		.get(crate::oauth::SESSION_COOKIE_NAME)
		.ok_or(EmbeddedError::Unauthorized)?;
	let principal = crate::sessions::resolve_browser(&state, login.value())
		.await
		.map_err(|_| EmbeddedError::Unauthorized)?;
	let crate::sessions::Principal::Browser(claims) = principal else {
		return Err(EmbeddedError::Unauthorized.into());
	};

	let public = feedback::public_project(&state, &input.organization, &input.project)
		.await?
		.ok_or(EmbeddedError::NotFound)?;
	let project = public.project.id;
	let subject = feedback::account_viewer_id(
		&state.jwt_secret,
		monochange_app_db::projects::GITHUB,
		claims.github_id,
		project,
	);
	let token = credential("mcfs", project);
	let time = now();
	let expires_at = time + 600;
	let credential_hash = fingerprint(&credential("mcfi", project));
	let salt = credential("identity", project);
	feedback::update(&state, &public.scope, |service| {
		if !service
			.embedded()
			.integrations
			.iter()
			.any(|app| app.id == INTEGRATION)
		{
			service.embedded_mut().register(
				Integration {
					id: INTEGRATION.into(),
					name: "Hosted feedback iframe".into(),
					namespace: "monochange-portal".into(),
					origins: allowed.clone(),
					surfaces: vec![ClientSurface::Web],
					credential_hash: credential_hash.clone(),
					generation: 0,
					revoked: false,
				},
				&salt,
			)?;
		}
		let app = service.embedded().integration(INTEGRATION)?.clone();
		service
			.embedded_mut()
			.quota("iframe:sessions", 1000, 3600, time)?;
		service
			.embedded_mut()
			.quota(&format!("sessions:{subject}"), 30, 3600, time)?;
		service.embedded_mut().issue_session(
			FeedbackSession {
				token_hash: fingerprint(&token),
				integration: app.id,
				subject: subject.clone(),
				surface: ClientSurface::Web,
				generation: app.generation,
				expires_at,
			},
			time,
		)?;
		Ok(())
	})
	.await?;
	Ok((
		[("cache-control", "no-store")],
		Json(json!({"token":token,"expires_at":expires_at,"project":project,"identity":subject})),
	)
		.into_response())
}
