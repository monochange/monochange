//! Release records gain optional `cut_base_commit` and `synced_from_commit`
//! fields in schema v0.11 to support frozen release cuts and post-release sync
//! without duplicating tags or losing cut ancestry when cut branches are
//! deleted after publish.
//!
//! Both fields are optional and default to `None` (omitted when absent), so
//! v0.10 payloads migrate unchanged. The edge exists so artifacts created under
//! v0.10 migrate to current while the published v0.10 contract stays frozen.

use serde_json::Value;

use crate::SchemaError;

/// The `Result` signature matches the migration-edge contract; the v0.11
/// change adds optional fields, so every payload migrates unchanged.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply(_value: &mut Value) -> Result<(), SchemaError> {
	Ok(())
}
