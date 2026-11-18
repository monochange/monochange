//! Repository connection capabilities for the signed-in dashboard.

#[cfg(test)]
#[path = "__tests__/installation_tests.rs"]
mod tests;

use leptos::server;
use leptoskit_rpc::api;
pub use monochange_shared::PermissionUpdate;
pub use monochange_shared::RepositoryConnectionStatus;

/// Resolve repository setup without exposing app credentials or installation tokens.
///
/// # Errors
/// Returns an error for an invalid session or a failed configured-app lookup.
#[api(crate = "leptoskit_rpc", tag = "installation")]
#[server(endpoint = "repository_connection")]
pub async fn repository_connection() -> Result<RepositoryConnectionStatus, server_fn::ServerFnError>
{
	use std::sync::Arc;

	use leptos::prelude::expect_context;
	use leptos_axum::ResponseOptions;

	// A stale browser cookie reads as signed out for a status display, not an
	// error page; a rejected credential still propagates as an error.
	if super::auth::get_session().await?.is_none() {
		return Ok(RepositoryConnectionStatus::SignedOut);
	}

	let state: Arc<monochange_app_api::AppState> = expect_context();
	let Some(app) = state.github_app.as_ref() else {
		return Ok(RepositoryConnectionStatus::Unavailable);
	};

	let installation_url = app.installation_url(&state.http).await.map_err(|error| {
		let message = match error {
			monochange_app_api::github_app::GitHubAppError::Status(_, status, _) => {
				format!("GitHub repository setup failed with status {status}")
			}
			_ => "GitHub repository setup could not be loaded".to_string(),
		};

		server_fn::ServerFnError::new(message)
	})?;
	let pending = monochange_app_api::oauth::installation_state(&state.jwt_secret)
		.map_err(|error| server_fn::ServerFnError::new(format!("OAuth state: {error}")))?;
	let response = expect_context::<ResponseOptions>();
	response.append_header(
		axum::http::header::SET_COOKIE,
		axum::http::HeaderValue::from_str(&pending.cookie.encoded().to_string())
			.map_err(|error| server_fn::ServerFnError::new(format!("Cookie: {error}")))?,
	);
	let mut installation_url = url::Url::parse(&installation_url)
		.map_err(|error| server_fn::ServerFnError::new(format!("Installation URL: {error}")))?;
	installation_url
		.query_pairs_mut()
		.append_pair("state", &pending.state);

	Ok(RepositoryConnectionStatus::Available {
		installation_url: installation_url.into(),
	})
}

/// Connected accounts whose installation still runs on older permissions.
///
/// GitHub keeps every installation on the permissions it accepted until an
/// owner of the account approves the app's new request. Signed-in users don't
/// need to sign in again: their access follows the installation once it is
/// approved. A failed GitHub lookup leaves the account out instead of failing
/// the dashboard.
///
/// # Errors
/// Returns an error for an invalid session or an unverifiable organisation.
#[api(crate = "leptoskit_rpc", tag = "installation")]
#[server(endpoint = "permission_updates")]
pub async fn permission_updates() -> Result<Vec<PermissionUpdate>, server_fn::ServerFnError> {
	use std::sync::Arc;

	use leptos::prelude::expect_context;

	let state: Arc<monochange_app_api::AppState> = expect_context();
	let Some(app) = state.github_app.as_ref() else {
		return Ok(vec![]);
	};
	let Some(user_id) = super::repos::signed_in_user_id().await? else {
		return Ok(vec![]);
	};
	let mut installations: Vec<(i64, String, bool)> =
		super::repos::accessible_repositories(&state, user_id)
			.await?
			.into_iter()
			.map(|repository| {
				(
					repository.github_installation_id,
					repository.info.installation_login,
					repository.account_type == "User",
				)
			})
			.collect();
	installations.sort();
	installations.dedup();

	if installations.is_empty() {
		return Ok(vec![]);
	}

	let requested = match app.requested_permissions(&state.http).await {
		Ok(requested) => requested,
		Err(error) => {
			tracing::warn!(%error, "app permission lookup failed");
			return Ok(vec![]);
		}
	};
	let mut updates = Vec::new();

	for (installation_id, account, personal) in installations {
		let granted = match app
			.installation_permissions(&state.http, installation_id)
			.await
		{
			Ok(granted) => granted,
			Err(error) => {
				tracing::warn!(%error, installation = installation_id, "installation permission lookup failed");
				continue;
			}
		};
		let missing: Vec<String> =
			monochange_app_api::github_app::pending_permissions(&requested, &granted)
				.iter()
				.filter(|pending| !personal || !is_organization_permission(&pending.name))
				.map(permission_label)
				.collect();

		if missing.is_empty() {
			continue;
		}

		updates.push(PermissionUpdate {
			review_url: installation_settings_url(&account, personal, installation_id),
			account,
			missing: missing.join(", "),
		});
	}

	Ok(updates)
}

/// The GitHub page where an account owner reviews an installation's
/// permission request.
#[cfg(not(target_arch = "wasm32"))]
fn installation_settings_url(account: &str, personal: bool, installation_id: i64) -> String {
	if personal {
		format!("https://github.com/settings/installations/{installation_id}")
	} else {
		format!(
			"https://github.com/organizations/{}/settings/installations/{installation_id}",
			urlencoding::encode(account)
		)
	}
}

/// Organization permissions such as `members` only apply to installations on
/// organizations; GitHub never grants them to a personal account, so a
/// personal installation can't be behind on them.
#[cfg(not(target_arch = "wasm32"))]
fn is_organization_permission(name: &str) -> bool {
	name == "members" || name.starts_with("organization_")
}

/// A short label such as `Pull requests: write (now read)`. Built on the
/// server so the dashboard's browser code only renders text.
#[cfg(not(target_arch = "wasm32"))]
fn permission_label(permission: &monochange_app_api::github_app::PendingPermission) -> String {
	let mut name = permission.name.replace('_', " ");
	if let Some(first) = name.get_mut(..1) {
		first.make_ascii_uppercase();
	}

	format!(
		"{name}: {} (now {})",
		permission.requested,
		permission.granted.as_deref().unwrap_or("none")
	)
}
