//! Intake types describing what a third-party app submits to the feedback
//! pipeline, and the registration that lets an app submit at all.

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// Longest description the pipeline accepts. Long enough for a detailed bug
/// report, short enough that one submission cannot flood triage or the
/// maintainer dashboard.
pub const MAX_DESCRIPTION_CHARS: usize = 4_000;

/// What the submitter believes they are reporting. Triage may reclassify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackKind {
	FeatureRequest,
	BugReport,
}

/// The on-screen element a user pointed at when they wrote the feedback.
///
/// Pinning feedback to an element is what turns "the export is broken" into a
/// reproducible report: triage gets the exact control instead of guessing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedElement {
	/// A selector the integrating app can resolve, such as `#export-csv`.
	pub selector: String,
	/// The element's visible text, such as `Export CSV`.
	pub label: Option<String>,
}

/// Where in the integrated app the feedback was captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageContext {
	/// App-defined route such as `/reports/quarterly`.
	pub route: String,
	pub app_version: Option<String>,
	pub locale: Option<String>,
	pub element: Option<PinnedElement>,
}

/// Evidence attached by the widget. Media bytes live in object storage; the
/// pipeline only ever sees opaque identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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

/// A third-party app allowed to submit feedback for one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredApp {
	pub slug: String,
	pub display_name: String,
	/// Exact origins (`scheme://host[:port]`) the embedded widget may submit
	/// from. There is no wildcard: an app that cannot list its origins cannot
	/// embed the widget.
	pub allowed_origins: Vec<String>,
}

impl RegisteredApp {
	/// Whether a browser `Origin` header belongs to this app. Comparison is
	/// exact apart from a trailing slash and ASCII case, which browsers never
	/// vary meaningfully.
	pub fn allows_origin(&self, origin: &str) -> bool {
		let origin = origin.trim_end_matches('/');
		self.allowed_origins
			.iter()
			.any(|allowed| allowed.trim_end_matches('/').eq_ignore_ascii_case(origin))
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackSubmission {
	pub kind: FeedbackKind,
	pub description: String,
	pub page: Option<PageContext>,
	pub attachments: Vec<Attachment>,
	pub submitter: SubmitterIdentity,
	/// Slug of the [`RegisteredApp`] the submission arrived through.
	pub app_slug: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IntakeError {
	#[error("the description is empty")]
	EmptyDescription,
	#[error("the description is longer than {MAX_DESCRIPTION_CHARS} characters")]
	DescriptionTooLong,
	#[error("the submitter id is empty")]
	MissingSubmitter,
}

impl FeedbackSubmission {
	/// Rejects submissions the pipeline cannot process. Runs before anything
	/// is stored so malformed input never reaches triage.
	pub fn validate(&self) -> Result<(), IntakeError> {
		if self.description.trim().is_empty() {
			return Err(IntakeError::EmptyDescription);
		}
		if self.description.chars().count() > MAX_DESCRIPTION_CHARS {
			return Err(IntakeError::DescriptionTooLong);
		}
		if self.submitter.anonymous_id.trim().is_empty() {
			return Err(IntakeError::MissingSubmitter);
		}
		Ok(())
	}
}

#[cfg(test)]
#[path = "__tests__/submission_tests.rs"]
mod tests;
