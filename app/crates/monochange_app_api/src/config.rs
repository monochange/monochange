//! GitHub App credentials from the resolved application secrets.

use jsonwebtoken::EncodingKey;

use crate::AppSecrets;
use crate::github_app::GitHubAppAuth;
use crate::github_app::GitHubAppError;

pub(super) fn github_app_credentials(
	secrets: &AppSecrets,
) -> Result<Option<GitHubAppAuth>, GitHubAppError> {
	let app_id = secrets
		.github_app_id
		.as_deref()
		.filter(|value| !value.trim().is_empty());
	let private_key = secrets
		.github_app_private_key
		.as_deref()
		.filter(|value| !value.trim().is_empty());
	let webhook_secret = secrets
		.github_app_webhook_secret
		.as_deref()
		.filter(|value| !value.trim().is_empty());
	match (app_id, private_key, webhook_secret) {
		(None, None, None) => Ok(None),
		(Some(app_id), Some(private_key), Some(webhook_secret)) => {
			EncodingKey::from_rsa_pem(private_key.as_bytes())?;
			let api_url = std::env::var("GITHUB_API_URL")
				.ok()
				.filter(|url| !url.is_empty())
				.unwrap_or_else(|| "https://api.github.com".to_string());
			Ok(Some(GitHubAppAuth::new(
				app_id,
				private_key,
				webhook_secret,
				&api_url,
			)))
		}
		_ => Err(GitHubAppError::NotConfigured),
	}
}

#[cfg(test)]
#[path = "__tests__/config_tests.rs"]
mod tests;
