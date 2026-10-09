use crate::submission::FeedbackKind;
use crate::submission::FeedbackSubmission;
use crate::submission::IntakeError;
use crate::submission::MAX_DESCRIPTION_CHARS;
use crate::tests::fixtures::app;
use crate::tests::fixtures::pinned;
use crate::tests::fixtures::submission;

#[test]
fn origins_match_exactly_ignoring_case_and_trailing_slash() {
	let app = app();
	assert!(app.allows_origin("https://invoices.example"));
	assert!(app.allows_origin("https://Invoices.Example/"));
	assert!(!app.allows_origin("https://invoices.example.evil.dev"));
	assert!(!app.allows_origin("http://invoices.example"));
}

#[test]
fn complete_submissions_validate() {
	assert_eq!(
		submission(FeedbackKind::BugReport, "Totals are wrong").validate(),
		Ok(())
	);
}

#[test]
fn blank_descriptions_are_rejected() {
	assert_eq!(
		submission(FeedbackKind::BugReport, "  \n\t ").validate(),
		Err(IntakeError::EmptyDescription)
	);
}

#[test]
fn oversized_descriptions_are_rejected() {
	let description = "a".repeat(MAX_DESCRIPTION_CHARS + 1);
	assert_eq!(
		submission(FeedbackKind::BugReport, &description).validate(),
		Err(IntakeError::DescriptionTooLong)
	);
	let at_limit = "a".repeat(MAX_DESCRIPTION_CHARS);
	assert_eq!(
		submission(FeedbackKind::BugReport, &at_limit).validate(),
		Ok(())
	);
}

#[test]
fn anonymous_submitters_still_need_an_id() {
	let mut submission = submission(FeedbackKind::FeatureRequest, "Dark mode");
	submission.submitter.anonymous_id = " ".to_owned();
	assert_eq!(submission.validate(), Err(IntakeError::MissingSubmitter));
}

#[test]
fn intake_errors_explain_themselves() {
	assert_eq!(
		IntakeError::EmptyDescription.to_string(),
		"the description is empty"
	);
	assert_eq!(
		IntakeError::DescriptionTooLong.to_string(),
		"the description is longer than 4000 characters"
	);
	assert_eq!(
		IntakeError::MissingSubmitter.to_string(),
		"the submitter id is empty"
	);
}

#[test]
fn submissions_round_trip_through_json() {
	let mut original = submission(FeedbackKind::BugReport, "Export fails");
	original.page = Some(pinned("/invoices", "#export-csv", Some("Export CSV")));
	original
		.attachments
		.push(crate::submission::Attachment::ScreenRecording {
			media_id: "rec-1".to_owned(),
		});
	let json = serde_json::to_value(&original).unwrap();
	assert_eq!(json["kind"], "bug_report");
	assert_eq!(json["attachments"][0]["type"], "screenshot");
	assert_eq!(json["page"]["element"]["selector"], "#export-csv");
	let decoded: FeedbackSubmission = serde_json::from_value(json).unwrap();
	assert_eq!(decoded, original);

	let app = app();
	let decoded_app: crate::submission::RegisteredApp =
		serde_json::from_value(serde_json::to_value(&app).unwrap()).unwrap();
	assert_eq!(decoded_app, app);
}
