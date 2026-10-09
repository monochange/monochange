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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
struct GitHubUserTokenResponse {
	access_token: Option<String>,
	expires_in: Option<i64>,
	refresh_token: Option<String>,
	refresh_token_expires_in: Option<i64>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct GitHubUserToken {
	access_token: String,
	access_token_expires_at: Option<i64>,
	refresh_token: Option<String>,
	refresh_token_expires_at: Option<i64>,
}

#[cfg(not(target_arch = "wasm32"))]
type StoredGitHubUserToken = (String, Option<String>, Option<i64>, Option<i64>);

#[cfg(not(target_arch = "wasm32"))]
impl GitHubUserTokenResponse {
	fn into_token(self) -> Result<GitHubUserToken, server_fn::ServerFnError> {
		let access_token = self
			.access_token
			.filter(|token| !token.is_empty())
			.ok_or_else(|| {
				server_fn::ServerFnError::new("GitHub did not return an access token")
			})?;
		let now = chrono::Utc::now().timestamp();
		let access_token_expires_at = self.expires_in.map(|seconds| now + seconds);
		let refresh_token = self.refresh_token.filter(|token| !token.is_empty());
		let refresh_token_expires_at = self.refresh_token_expires_in.map(|seconds| now + seconds);

		if access_token_expires_at.is_some()
			&& (refresh_token.is_none() || refresh_token_expires_at.is_none())
		{
			return Err(server_fn::ServerFnError::new(
				"GitHub returned an expiring token without refresh credentials",
			));
		}

		Ok(GitHubUserToken {
			access_token,
			access_token_expires_at,
			refresh_token,
			refresh_token_expires_at,
		})
	}
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
	let pending = monochange_app_api::oauth::login_state(&state.jwt_secret)
		.map_err(|error| server_fn::ServerFnError::new(format!("OAuth state: {error}")))?;
	let resp = expect_context::<ResponseOptions>();
	resp.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&pending.cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	let mut url = url::Url::parse(&format!(
		"{}/login/oauth/authorize",
		state.github_oauth_origin.trim_end_matches('/'),
	))
	.map_err(|error| server_fn::ServerFnError::new(format!("OAuth URL: {error}")))?;
	{
		let mut query = url.query_pairs_mut();
		query
			.append_pair("client_id", &state.github_client_id)
			.append_pair("redirect_uri", &state.github_callback_url)
			.append_pair("state", &pending.state);
		if let Some(challenge) = pending.code_challenge {
			query
				.append_pair("code_challenge", &challenge)
				.append_pair("code_challenge_method", "S256");
		}
	}
	Ok(url.into())
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
	let verified =
		monochange_app_api::oauth::verify_login_state(&state.jwt_secret, &jar, &state_param)
			.map_err(|_| server_fn::ServerFnError::new("Invalid or expired OAuth state"))?;
	let resp = expect_context::<ResponseOptions>();
	let cookie = monochange_app_api::oauth::clear_login_state();
	resp.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);

	let mut token_form = vec![
		("client_id", state.github_client_id.clone()),
		("client_secret", state.github_client_secret.clone()),
		("code", code),
		("redirect_uri", state.github_callback_url.clone()),
	];
	if let Some(verifier) = verified.code_verifier {
		token_form.push(("code_verifier", verifier));
	}
	let token_response = state
		.http
		.post(format!(
			"{}/login/oauth/access_token",
			state.github_oauth_origin.trim_end_matches('/'),
		))
		.header("Accept", "application/json")
		.form(&token_form)
		.send()
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("Token: {e}")))?
		.json::<GitHubUserTokenResponse>()
		.await
		.map_err(|e| server_fn::ServerFnError::new(format!("Token response: {e}")))?
		.into_token()?;

	let gh_user: serde_json::Value = state
		.http
		.get(format!("{}/user", state.github_api_origin))
		.header(
			"Authorization",
			format!("Bearer {}", token_response.access_token),
		)
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
		"INSERT INTO users (
			github_id, github_login, github_avatar_url, github_access_token,
			github_refresh_token, github_access_token_expires_at,
			github_refresh_token_expires_at
		 ) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT(github_id) DO UPDATE SET
		 github_login = excluded.github_login, github_avatar_url = excluded.github_avatar_url,
		 github_access_token = excluded.github_access_token,
		 github_refresh_token = excluded.github_refresh_token,
		 github_access_token_expires_at = excluded.github_access_token_expires_at,
		 github_refresh_token_expires_at = excluded.github_refresh_token_expires_at,
		 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') RETURNING id, plan_tier",
	)
	.bind(github_id)
	.bind(&login)
	.bind(&avatar)
	.bind(&token_response.access_token)
	.bind(&token_response.refresh_token)
	.bind(token_response.access_token_expires_at)
	.bind(token_response.refresh_token_expires_at)
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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn github_user_access_token(
	state: &monochange_app_api::AppState,
	user_id: i32,
) -> Result<String, server_fn::ServerFnError> {
	let _refresh_guard = state.github_token_refresh_lock.lock().await;
	let stored: Option<StoredGitHubUserToken> = sqlx::query_as(
		"SELECT github_access_token, github_refresh_token,
		        github_access_token_expires_at, github_refresh_token_expires_at
		 FROM users WHERE id = $1",
	)
	.bind(user_id)
	.fetch_optional(&state.db)
	.await
	.map_err(|error| server_fn::ServerFnError::new(format!("DB: {error}")))?;
	let Some((access_token, refresh_token, access_expires_at, refresh_expires_at)) = stored else {
		return Err(server_fn::ServerFnError::new("Invalid session"));
	};
	if access_token.trim().is_empty() {
		return Err(server_fn::ServerFnError::new(
			"please sign in again to verify organization access",
		));
	}

	let now = chrono::Utc::now().timestamp();
	if access_expires_at.is_none_or(|expires_at| expires_at > now + 60) {
		return Ok(access_token);
	}
	let refresh_token = refresh_token
		.filter(|token| !token.is_empty())
		.filter(|_| refresh_expires_at.is_some_and(|expires_at| expires_at > now + 60))
		.ok_or_else(|| {
			server_fn::ServerFnError::new("please sign in again to verify organization access")
		})?;
	let token_form = [
		("client_id", state.github_client_id.clone()),
		("client_secret", state.github_client_secret.clone()),
		("grant_type", "refresh_token".to_string()),
		("refresh_token", refresh_token),
	];
	let refreshed = state
		.http
		.post(format!(
			"{}/login/oauth/access_token",
			state.github_oauth_origin.trim_end_matches('/'),
		))
		.header("Accept", "application/json")
		.form(&token_form)
		.send()
		.await
		.map_err(|error| {
			tracing::warn!(%error, "GitHub user token refresh failed");
			server_fn::ServerFnError::new("please sign in again to verify organization access")
		})?
		.json::<GitHubUserTokenResponse>()
		.await
		.map_err(|error| {
			tracing::warn!(%error, "GitHub user token refresh response was invalid");
			server_fn::ServerFnError::new("please sign in again to verify organization access")
		})?
		.into_token()
		.map_err(|_| {
			server_fn::ServerFnError::new("please sign in again to verify organization access")
		})?;

	sqlx::query(
		"UPDATE users SET github_access_token = $1, github_refresh_token = $2,
		 github_access_token_expires_at = $3, github_refresh_token_expires_at = $4,
		 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = $5",
	)
	.bind(&refreshed.access_token)
	.bind(&refreshed.refresh_token)
	.bind(refreshed.access_token_expires_at)
	.bind(refreshed.refresh_token_expires_at)
	.bind(user_id)
	.execute(&state.db)
	.await
	.map_err(|error| server_fn::ServerFnError::new(format!("Save user token: {error}")))?;

	Ok(refreshed.access_token)
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
