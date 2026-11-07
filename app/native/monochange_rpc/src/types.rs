//! Wire types shared by every monochange leptoskit surface.
//!
//! Shapes mirror the existing Leptos 0.8 `SessionUser` / project summary
//! fields so the web port can migrate without renaming JSON keys.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Public session data returned by the auth handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionUser {
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
}

/// Result of probing whether the caller has a usable session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthHandshake {
	/// `None` when the caller is anonymous.
	pub session: Option<SessionUser>,
	/// Stable protocol label so native and web can negotiate.
	pub protocol: String,
	/// Human-readable status for the shell UI.
	pub status: String,
}

impl AuthHandshake {
	pub const PROTOCOL: &'static str = "monochange.rpc.v1";

	#[must_use]
	pub fn anonymous() -> Self {
		Self {
			session: None,
			protocol: Self::PROTOCOL.to_owned(),
			status: "anonymous".to_owned(),
		}
	}

	#[must_use]
	pub fn signed_in(session: SessionUser) -> Self {
		Self {
			session: Some(session),
			protocol: Self::PROTOCOL.to_owned(),
			status: "signed_in".to_owned(),
		}
	}
}

/// One project row for the native project floor list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectSummary {
	pub organization: String,
	pub slug: String,
	pub name: String,
	pub description: String,
	pub repository_count: i64,
}
