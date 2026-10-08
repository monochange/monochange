//! Intake types describing what a third-party app submits to the feedback
//! pipeline.

use serde::Deserialize;
use serde::Serialize;

/// What the submitter believes they are reporting. Triage may reclassify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackKind {
	FeatureRequest,
	BugReport,
}

/// Where in the integrated app the feedback was captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageContext {
	/// App-defined route such as `/reports/quarterly`.
	pub route: String,
	pub app_version: Option<String>,
	pub locale: Option<String>,
}

/// Evidence attached by the widget. Media bytes live in object storage; the
/// pipeline only ever sees opaque identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attachment {
	Screenshot { media_id: String },
	ScreenRecording { media_id: String },
}

/// Pseudonymous submitter identity. The anonymous id is a stable hash so
/// votes can be deduplicated without storing raw contact details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitterIdentity {
	pub anonymous_id: String,
	pub email: Option<String>,
}

/// The integrated app (feedback form) the submission arrived through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredApp {
	pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackSubmission {
	pub id: String,
	pub kind: FeedbackKind,
	pub description: String,
	pub page: Option<PageContext>,
	pub attachments: Vec<Attachment>,
	pub submitter: SubmitterIdentity,
	pub app: RegisteredApp,
}

#[cfg(test)]
#[path = "__tests__/submission_tests.rs"]
mod tests;
