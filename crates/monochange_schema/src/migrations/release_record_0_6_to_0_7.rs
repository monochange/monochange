//! Release-record payloads are unchanged in schema v0.7; the version advanced
//! because `[changesets.classification]` joined the published configuration
//! contract with a new `skip_labels` default. The edge exists so artifacts
//! created under v0.6 migrate to current, and so the 0.6 contract stays frozen.

use serde_json::Value;

use crate::SchemaError;

/// The `Result` signature matches the migration-edge contract; the v0.7
/// change alters no record data, so every payload migrates unchanged.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply(_value: &mut Value) -> Result<(), SchemaError> {
	Ok(())
}
