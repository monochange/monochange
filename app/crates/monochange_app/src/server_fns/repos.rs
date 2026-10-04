//! Repository management server functions.

use leptos::server;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
	pub id: i32,
	pub github_full_name: String,
	pub github_private: bool,
	pub plan_tier: String,
	/// GitHub App installation account, e.g. `monochange`.
	pub installation_login: String,
	/// Whether the installation is active (`selected`) or suspended.
	pub installation_suspended: bool,
}

#[server]
pub async fn list_repos() -> Result<Vec<RepoInfo>, server_fn::ServerFnError> {
	use std::sync::Arc;

	use axum_extra::extract::cookie::CookieJar;
	use leptos_axum::extract;
	use sqlx::Row;

	let jar: CookieJar = extract().await?;
	let Some(token) = jar.get("monochange_session").map(|c| c.value().to_string()) else {
		return Ok(vec![]);
	};

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();

	let claims = monochange_app_api::verify_token(&state.jwt_secret, &token)
		.map_err(|_| server_fn::ServerFnError::new("Invalid session"))?;

	let user_id: i32 = claims.sub;

	let rows = sqlx::query(
		"SELECT r.id, r.github_full_name, r.github_private, r.plan_tier,
		        i.github_account_login, i.target_type
		 FROM repositories r
		 JOIN installations i ON r.installation_id = i.id
		 WHERE i.user_id = $1",
	)
	.bind(user_id)
	.fetch_all(&state.db)
	.await
	.map_err(|e| server_fn::ServerFnError::new(format!("DB: {e}")))?;

	let repos = rows
		.into_iter()
		.map(|r| {
			RepoInfo {
				id: r.get("id"),
				github_full_name: r.get("github_full_name"),
				github_private: r.get("github_private"),
				plan_tier: r.get("plan_tier"),
				installation_login: r.get("github_account_login"),
				installation_suspended: r.get::<String, _>("target_type") == "suspended",
			}
		})
		.collect();

	Ok(repos)
}

// Leptos server functions must be async.
#[allow(clippy::unused_async)]
#[server]
pub async fn get_repo(full_name: String) -> Result<Option<RepoInfo>, server_fn::ServerFnError> {
	let _ = full_name;
	Ok(None)
}
