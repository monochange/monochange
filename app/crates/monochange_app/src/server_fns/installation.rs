//! Repository connection capabilities for the signed-in dashboard.

#[cfg(test)]
#[path = "__tests__/installation_tests.rs"]
mod tests;

use leptos::server;
use serde::Deserialize;
use serde::Serialize;

/// Whether the current visitor can connect repositories through the GitHub App.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryConnectionStatus {
	/// The visitor must establish a website session before connecting repositories.
	SignedOut,
	/// This deployment has no configured GitHub App.
	Unavailable,
	/// GitHub has confirmed the configured app's installation page.
	Available {
		/// Trusted GitHub page where the user chooses an account and repositories.
		installation_url: String,
	},
}

/// Resolve repository setup without exposing app credentials or installation tokens.
///
/// # Errors
/// Returns an error for an invalid session or a failed configured-app lookup.
#[server]
pub async fn repository_connection() -> Result<RepositoryConnectionStatus, server_fn::ServerFnError>
{
	use std::sync::Arc;

	use leptos::prelude::expect_context;

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

	Ok(RepositoryConnectionStatus::Available { installation_url })
}
