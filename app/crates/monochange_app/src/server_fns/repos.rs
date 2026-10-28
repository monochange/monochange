//! Repository management server functions.

#[cfg(test)]
#[path = "__tests__/repos_tests.rs"]
mod tests;

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

/// A repository the signed-in user may manage, with the identities projects
/// and organisations are keyed by.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub(crate) struct AccessibleRepository {
	pub info: RepoInfo,
	pub github_repo_id: i64,
	pub installation_id: i32,
	pub github_installation_id: i64,
	pub account_type: String,
	pub organization_id: Option<i32>,
}

/// The signed-in user's id, or `None` without a session cookie.
///
/// # Errors
/// Returns `Invalid session` for a forged, expired, or orphaned token.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn signed_in_user_id(
	state: &monochange_app_api::AppState,
) -> Result<Option<i32>, server_fn::ServerFnError> {
	use axum_extra::extract::cookie::CookieJar;
	use leptos_axum::extract;

	let jar: CookieJar = extract().await?;
	let Some(token) = jar
		.get(monochange_app_api::oauth::SESSION_COOKIE_NAME)
		.map(|cookie| cookie.value().to_string())
	else {
		return Ok(None);
	};
	let claims = monochange_app_api::verify_token(&state.jwt_secret, &token)
		.map_err(|_| server_fn::ServerFnError::new("Invalid session"))?;
	let user: Option<i64> = sqlx::query_scalar("SELECT github_id FROM users WHERE id = $1")
		.bind(claims.sub)
		.fetch_optional(&state.db)
		.await
		.map_err(|error| server_fn::ServerFnError::new(format!("DB: {error}")))?;

	match user {
		Some(github_id) if github_id == claims.github_id => Ok(Some(claims.sub)),
		_ => Err(server_fn::ServerFnError::new("Invalid session")),
	}
}

/// Every repository the user installed, minus organisation repositories they
/// no longer own.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn accessible_repositories(
	state: &monochange_app_api::AppState,
	user_id: i32,
) -> Result<Vec<AccessibleRepository>, server_fn::ServerFnError> {
	use std::collections::HashMap;

	use sqlx::Row;

	let rows = sqlx::query(
		"SELECT r.id, r.github_repo_id, r.github_full_name, r.github_private, r.plan_tier,
		        i.id AS installation_id, i.github_installation_id, i.github_account_login,
		        i.github_account_type, i.target_type, i.organization_id
		 FROM repositories r
		 JOIN installations i ON r.installation_id = i.id
		 WHERE i.user_id = $1 AND i.provider = $2 AND r.provider = $2
		 ORDER BY r.github_full_name COLLATE NOCASE",
	)
	.bind(user_id)
	.bind(monochange_app_db::projects::GITHUB)
	.fetch_all(&state.db)
	.await
	.map_err(|e| server_fn::ServerFnError::new(format!("DB: {e}")))?;

	let mut candidates: Vec<_> = rows
		.into_iter()
		.map(|row| {
			AccessibleRepository {
				info: RepoInfo {
					id: row.get("id"),
					github_full_name: row.get("github_full_name"),
					github_private: row.get("github_private"),
					plan_tier: row.get("plan_tier"),
					installation_login: row.get("github_account_login"),
					installation_suspended: row.get::<String, _>("target_type") == "suspended",
				},
				github_repo_id: row.get("github_repo_id"),
				installation_id: row.get("installation_id"),
				github_installation_id: row.get("github_installation_id"),
				account_type: row.get("github_account_type"),
				organization_id: row.get("organization_id"),
			}
		})
		.collect();
	link_legacy_installations(state, &mut candidates).await?;
	let mut repos = Vec::with_capacity(candidates.len());
	let mut organizations = HashMap::new();
	let mut access_token = None;

	for repository in candidates {
		if repository.account_type == "User" {
			repos.push(repository);
			continue;
		}

		if repository.account_type != "Organization" {
			continue;
		}

		let Some(organization_id) = repository.organization_id else {
			continue;
		};
		let installation_login = &repository.info.installation_login;
		let identity = (organization_id, installation_login.clone());
		let allowed = if let Some(allowed) = organizations.get(&identity) {
			*allowed
		} else {
			let Some(organization) = monochange_app_db::projects::find_organization_by_id(
				&state.db,
				monochange_app_db::projects::GITHUB,
				organization_id,
			)
			.await
			.map_err(|error| server_fn::ServerFnError::new(format!("DB: {error}")))?
			else {
				continue;
			};
			let token = match access_token {
				Some(ref token) => token,
				None => {
					access_token
						.insert(super::auth::github_user_access_token(state, user_id).await?)
				}
			};
			let allowed =
				organization_owner(state, token, installation_login, organization.external_id)
					.await?;
			organizations.insert(identity, allowed);
			allowed
		};

		if allowed {
			repos.push(repository);
		}
	}

	Ok(repos)
}

/// Links legacy installations before their provider identity authorizes access.
/// Failed lookups remain unlinked and cannot authorize organization access.
#[cfg(not(target_arch = "wasm32"))]
async fn link_legacy_installations(
	state: &monochange_app_api::AppState,
	repositories: &mut [AccessibleRepository],
) -> Result<(), server_fn::ServerFnError> {
	let Some(app) = state.github_app.as_ref() else {
		return Ok(());
	};
	let mut unlinked: Vec<(i32, i64)> = repositories
		.iter()
		.filter(|repository| repository.organization_id.is_none())
		.map(|repository| {
			(
				repository.installation_id,
				repository.github_installation_id,
			)
		})
		.collect();
	unlinked.sort_unstable();
	unlinked.dedup();

	for (installation_id, github_installation_id) in unlinked {
		let account = match app
			.installation_account(&state.http, github_installation_id)
			.await
		{
			Ok(account) => account,
			Err(error) => {
				tracing::warn!(%error, installation = github_installation_id, "installation account lookup failed");
				continue;
			}
		};
		let mut connection = state
			.db
			.acquire()
			.await
			.map_err(|error| server_fn::ServerFnError::new(format!("DB: {error}")))?;
		let organization_id = monochange_app_db::projects::link_installation_organization(
			&mut connection,
			installation_id,
			&account,
		)
		.await
		.map_err(|error| server_fn::ServerFnError::new(format!("DB: {error}")))?;

		for repository in repositories
			.iter_mut()
			.filter(|repository| repository.installation_id == installation_id)
		{
			repository.organization_id = Some(organization_id);
		}
	}

	Ok(())
}

#[server]
pub async fn list_repos() -> Result<Vec<RepoInfo>, server_fn::ServerFnError> {
	use std::sync::Arc;

	let state: Arc<monochange_app_api::AppState> = leptos::prelude::expect_context();
	let Some(user_id) = signed_in_user_id(&state).await? else {
		return Ok(vec![]);
	};
	Ok(accessible_repositories(&state, user_id)
		.await?
		.into_iter()
		.map(|repository| repository.info)
		.collect())
}

// An installer can leave an organization while its app remains installed.
// Verify their current owner role rather than treating that old link as access.
// Bind the returned account ID too, because GitHub logins can be reused.
#[cfg(not(target_arch = "wasm32"))]
async fn organization_owner(
	state: &monochange_app_api::AppState,
	access_token: &str,
	organization: &str,
	expected_id: i64,
) -> Result<bool, server_fn::ServerFnError> {
	#[derive(Deserialize)]
	struct Membership {
		state: String,
		role: String,
		organization: MembershipOrganization,
	}

	#[derive(Deserialize)]
	struct MembershipOrganization {
		id: i64,
	}

	if access_token.trim().is_empty() {
		return Err(server_fn::ServerFnError::new(
			"please sign in again to verify organization access",
		));
	}

	let response = state
		.http
		.get(format!(
			"{}/user/memberships/orgs/{}",
			state.github_api_origin.trim_end_matches('/'),
			urlencoding::encode(organization),
		))
		.bearer_auth(access_token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange-app")
		.send()
		.await
		.map_err(|error| {
			tracing::warn!(%error, "organization membership request failed");
			server_fn::ServerFnError::new("organization access could not be verified")
		})?;

	if response.status() == reqwest::StatusCode::NOT_FOUND {
		return Ok(false);
	}

	if response.status() == reqwest::StatusCode::UNAUTHORIZED {
		return Err(server_fn::ServerFnError::new(
			"please sign in again to verify organization access",
		));
	}

	if !response.status().is_success() {
		tracing::warn!(status = %response.status(), "organization membership request rejected");
		return Err(server_fn::ServerFnError::new(
			"organization access could not be verified",
		));
	}

	let membership: Membership = response.json().await.map_err(|error| {
		tracing::warn!(%error, "organization membership response was invalid");
		server_fn::ServerFnError::new("organization access could not be verified")
	})?;

	Ok(membership.state == "active"
		&& membership.role == "admin"
		&& membership.organization.id == expected_id)
}

#[server]
pub async fn get_repo(full_name: String) -> Result<Option<RepoInfo>, server_fn::ServerFnError> {
	Ok(list_repos()
		.await?
		.into_iter()
		.find(|repository| repository.github_full_name == full_name))
}
