//! Book chapters delivered to the website shell.

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChapterLink {
	pub title: String,
	pub href: String,
	pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BookChapter {
	pub title: String,
	pub href: String,
	pub source: String,
	pub section: String,
	/// Generated and sanitized on the server.
	pub html: String,
	pub previous: Option<ChapterLink>,
	pub next: Option<ChapterLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BookPage {
	pub chapter: BookChapter,
	pub navigation: Vec<ChapterLink>,
}
