//! Authentication server functions with database integration.

#[cfg(test)]
#[path = "__tests__/auth_tests.rs"]
mod tests;

use leptos::prelude::*;
use leptos::server;
use serde::Deserialize;
use serde::Serialize;

/// Public session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
}

#[server]
pub async fn get_session() -> Result<Option<SessionUser>, server_fn::ServerFnError> {
	use std::sync::Arc;

	use axum_extra::extract::cookie::CookieJar;
	use leptos_axum::extract;

	let jar: CookieJar = extract().await?;
	let Some(token) = jar
		.get(monochange_app_api::oauth::SESSION_COOKIE_NAME)
		.map(|c| c.value().to_string())
	else {
		return Ok(None);
	};

	let state: Arc<monochange_app_api::AppState> = expect_context();
	let claims = monochange_app_api::verify_token(&state.jwt_secret, &token)
		.map_err(|_| server_fn::ServerFnError::new("Invalid session"))?;

	let client = monochange_app_db::get_client(&state.db)
		.map_err(|e| server_fn::ServerFnError::new(format!("DB: {e}")))?;

	let users = monochange_app_db::models::User::where_col(|u| u.id.equal(claims.sub))
		.run(&client)
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("Query: {e}")))?;

	Ok(users
		.first()
		.filter(|user| user.github_id == claims.github_id)
		.map(|u| {
			SessionUser {
				github_id: u.github_id,
				github_login: u.github_login.clone(),
				github_avatar_url: u.github_avatar_url.clone(),
				plan_tier: u.plan_tier.clone(),
			}
		}))
}

// Leptos server functions must be async.
#[allow(clippy::unused_async)]
#[server]
pub async fn get_login_url() -> Result<String, server_fn::ServerFnError> {
	use std::sync::Arc;

	use leptos_axum::ResponseOptions;

	let state: Arc<monochange_app_api::AppState> = expect_context();
	let (nonce, cookie) = monochange_app_api::oauth::login_state(&state.jwt_secret)
		.map_err(|error| server_fn::ServerFnError::new(format!("OAuth state: {error}")))?;
	let resp = expect_context::<ResponseOptions>();
	resp.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	Ok(format!(
		"{}/login/oauth/authorize?client_id={}&state={}&scope=user:email,read:org",
		state.github_oauth_origin, state.github_client_id, nonce,
	))
}

#[server]
pub async fn exchange_code(
	code: String,
	state_param: String,
) -> Result<SessionUser, server_fn::ServerFnError> {
	use std::sync::Arc;

	use axum_extra::extract::cookie::CookieJar;
	use leptos_axum::ResponseOptions;
	use leptos_axum::extract;

	let state: Arc<monochange_app_api::AppState> = expect_context();
	let jar: CookieJar = extract().await?;
	monochange_app_api::oauth::verify_login_state(&state.jwt_secret, &jar, &state_param)
		.map_err(|_| server_fn::ServerFnError::new("Invalid or expired OAuth state"))?;
	let resp = expect_context::<ResponseOptions>();
	let cookie = monochange_app_api::oauth::clear_login_state();
	resp.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	// Exchange code for access token
	let http = &state.http;
	let token_response: serde_json::Value = http
		.post(format!(
			"{}/login/oauth/access_token",
			state.github_oauth_origin
		))
		.header("Accept", "application/json")
		.json(&serde_json::json!({
			"client_id": state.github_client_id,
			"client_secret": state.github_client_secret,
			"code": code,
		}))
		.send()
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("Token: {e}")))?
		.json()
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("Parse: {e}")))?;

	let access_token = token_response["access_token"]
		.as_str()
		.ok_or_else(|| server_fn::ServerFnError::new("No access_token"))?
		.to_string();

	let gh_user: serde_json::Value = http
		.get(format!("{}/user", state.github_api_origin))
		.header("Authorization", format!("Bearer {access_token}"))
		.header("User-Agent", "monochange-app")
		.send()
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("User: {e}")))?
		.json()
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("Parse: {e}")))?;

	let github_id = gh_user["id"]
		.as_i64()
		.filter(|id| *id > 0)
		.ok_or_else(|| server_fn::ServerFnError::new("Invalid GitHub user"))?;
	let login = gh_user["login"]
		.as_str()
		.filter(|login| !login.is_empty())
		.ok_or_else(|| server_fn::ServerFnError::new("Invalid GitHub user"))?
		.to_string();
	let avatar = gh_user["avatar_url"].as_str().map(String::from);

	// OAuth and installation webhooks can arrive together for the same user.
	let (user_id, plan_tier): (i32, String) = sqlx::query_as(
		"INSERT INTO users (github_id, github_login, github_avatar_url, github_access_token)
		 VALUES ($1, $2, $3, $4) ON CONFLICT(github_id) DO UPDATE SET
		 github_login = excluded.github_login, github_avatar_url = excluded.github_avatar_url,
		 github_access_token = excluded.github_access_token,
		 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') RETURNING id, plan_tier",
	)
	.bind(github_id)
	.bind(&login)
	.bind(&avatar)
	.bind(access_token)
	.fetch_one(&state.db)
	.await
	.map_err(|error| server_fn::ServerFnError::new(format!("Save user: {error}")))?;

	// JWT
	let token = monochange_app_api::create_token(&state.jwt_secret, user_id, github_id, &login)
		.map_err(|e| server_fn::ServerFnError::new(format!("JWT: {e}")))?;

	// Cookie
	let cookie = monochange_app_api::oauth::session_cookie(token);

	resp.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	leptos_axum::redirect("/dashboard");
	Ok(SessionUser {
		github_id,
		github_login: login,
		github_avatar_url: avatar,
		plan_tier,
	})
}

// Leptos server functions must be async.
#[allow(clippy::unused_async)]
#[server]
pub async fn logout() -> Result<(), server_fn::ServerFnError> {
	use leptos_axum::ResponseOptions;

	let resp = expect_context::<ResponseOptions>();
	let cookie = monochange_app_api::oauth::session_cookie(String::new());

	resp.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	leptos_axum::redirect("/");
	Ok(())
}
