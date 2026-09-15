//! JSON Schema generation for the classification report.

use schemars::Schema;

use crate::classification::ChangeClassificationReport;

/// Generate the JSON Schema for the `monochange change classify` report.
pub fn classification_report() -> Schema {
	schemars::schema_for!(ChangeClassificationReport)
}
