//! Release-record targets gain `flow` in schema v0.10 so npm packages can
//! publish through npm staged publishing (`npm stage publish`) and the
//! release decision stays auditable in the committed record.
//!
//! The field is optional with a `direct` default, so v0.9 payloads migrate
//! unchanged: deserializing a v0.9 record yields `flow = "direct"`, matching
//! the publish behavior of every record created before staged publishing. The
//! edge exists so artifacts created under v0.9 migrate to current, and so the
//! 0.9 contract stays frozen.

use serde_json::Value;

use crate::SchemaError;

/// The `Result` signature matches the migration-edge contract; the v0.10
/// change adds an optional field, so every payload migrates unchanged.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply(_value: &mut Value) -> Result<(), SchemaError> {
	Ok(())
}
