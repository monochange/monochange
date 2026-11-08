//! Session identity visible to product clients.

use serde::Deserialize;
use serde::Serialize;

/// Public session data for a signed-in GitHub user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SessionUser {
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
}
