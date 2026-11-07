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
