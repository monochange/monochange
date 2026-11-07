//! Website-owned presentation of structured release notes.

use serde::Deserialize;
use serde::Serialize;

/// A published website version and its user-facing sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebsiteRelease {
	pub version: String,
	pub summary: Vec<String>,
	pub sections: Vec<WebsiteSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebsiteSection {
	pub title: String,
	pub entries: Vec<WebsiteEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebsiteEntry {
	pub summary: String,
	/// Sanitized on the server before crossing the rendering boundary.
	pub details_html: String,
}
