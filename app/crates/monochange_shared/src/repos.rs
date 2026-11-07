//! Repository summaries shown in the dashboard.

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
