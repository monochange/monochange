//! Release-record payloads are unchanged in schema v0.6; the version advanced
//! because the published `publish.timeout.timeout_seconds` default moved from
//! `60` to `300`. The edge exists so artifacts created under v0.5 migrate to
//! current, and so the 0.5 contract stays frozen.

use serde_json::Value;

use crate::SchemaError;

/// The `Result` signature matches the migration-edge contract; the v0.6
/// change alters no record data, so every payload migrates unchanged.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply(_value: &mut Value) -> Result<(), SchemaError> {
	Ok(())
}
