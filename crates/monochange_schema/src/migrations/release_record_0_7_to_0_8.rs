//! Release-record payloads are unchanged in schema v0.8; the version advanced
//! because the published configuration contract dropped
//! `[changelog.style].package_label_placement` and its release-notes override.
//!
//! Affected packages are now always rendered as a `_Packages:_` metadata line
//! directly above the owner line, so the placement setting no longer had
//! distinct values to select. The edge exists so artifacts created under v0.7
//! migrate to current, and so the 0.7 contract stays frozen.

use serde_json::Value;

use crate::SchemaError;

/// The `Result` signature matches the migration-edge contract; the v0.8
/// change alters no record data, so every payload migrates unchanged.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply(_value: &mut Value) -> Result<(), SchemaError> {
	Ok(())
}
