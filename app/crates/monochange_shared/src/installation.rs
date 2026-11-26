//! GitHub App installation status for the dashboard.

use serde::Deserialize;
use serde::Serialize;

/// Whether the current visitor can connect repositories through the GitHub App.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// A connected account whose GitHub App installation hasn't approved the
/// app's current permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PermissionUpdate {
	/// The GitHub account login, e.g. `monochange`.
	pub account: String,
	/// GitHub page where an owner of the account reviews the request.
	pub review_url: String,
	/// The missing access, e.g. `Actions: write (now none), Checks: write (now read)`.
	pub missing: String,
}

/// A connected repository whose recent merge velocity crossed the high-frequency
/// threshold (>1 merged PR every 10 minutes over at least 1 hour within the last
/// 24 hours), making it a strong candidate for frozen release cuts (`strategy = "cut"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReleaseCutSuggestion {
	/// Full `owner/repo` name on GitHub.
	pub repository: String,
	/// Highest count of merged PRs observed in any 60-minute sliding window in the last 24 hours.
	pub peak_prs_in_hour: usize,
	/// Total merged PRs observed into the default branch in the last 24 hours.
	pub merged_prs_24h: usize,
	/// Human-readable cadence summary for the peak window (e.g. `8 PRs in 1 hour (~1 every 7.5 min)`).
	pub burst_summary: String,
	/// Relative guide URL explaining how frozen release cuts work.
	pub guide_url: String,
	/// Self-contained prompt a developer can copy and paste into their coding agent.
	pub agent_prompt: String,
}

/// Outcome of the one-click action that opens or updates a setup pull request
/// enabling `strategy = "cut"` on a connected repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReleaseCutSetupResult {
	/// Full `owner/repo` name on GitHub.
	pub repository: String,
	/// Pull request number on GitHub.
	pub pull_request_number: u64,
	/// Pull request URL on GitHub.
	pub pull_request_url: String,
	/// Head branch created or updated for the setup pull request.
	pub branch: String,
	/// Whether a new pull request was opened (`true`) or an existing setup PR was updated (`false`).
	pub created: bool,
	/// Whether `.github/workflows/release-cut.yml` was committed directly (`true`) or included in the PR body because the installation lacks `workflows: write` (`false`).
	pub workflow_committed: bool,
}
