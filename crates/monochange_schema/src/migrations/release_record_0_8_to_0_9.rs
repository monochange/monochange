//! Release-record targets gain `rendered_title` and
//! `rendered_changelog_title` in schema v0.9 so provider releases published
//! from a committed record replay the exact title rendered at prepare time
//! instead of falling back to the tag name.
//!
//! Both fields are optional with empty-string defaults, so v0.8 payloads
//! migrate unchanged: deserializing a v0.8 record yields empty titles, and
//! publishing synthesizes the built-in default title from the record's target
//! id, version, and `created_at` date. The edge exists so artifacts created
//! under v0.8 migrate to current, and so the 0.8 contract stays frozen.

use serde_json::Value;

use crate::SchemaError;

/// The `Result` signature matches the migration-edge contract; the v0.9
/// change adds optional fields, so every payload migrates unchanged.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply(_value: &mut Value) -> Result<(), SchemaError> {
	Ok(())
}
