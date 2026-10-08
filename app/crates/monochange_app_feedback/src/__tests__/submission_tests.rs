use crate::submission::Attachment;
use crate::submission::FeedbackKind;
use crate::submission::FeedbackSubmission;
use crate::submission::PageContext;
use crate::submission::RegisteredApp;
use crate::submission::SubmitterIdentity;

fn sample_submission() -> FeedbackSubmission {
	FeedbackSubmission {
		id: "fb-1".to_owned(),
		kind: FeedbackKind::BugReport,
		description: "Totals are wrong on the quarterly report".to_owned(),
		page: Some(PageContext {
			route: "/reports/quarterly".to_owned(),
			app_version: Some("2.3.1".to_owned()),
			locale: Some("en-GB".to_owned()),
		}),
		attachments: vec![Attachment::Screenshot {
			media_id: "media-9".to_owned(),
		}],
		submitter: SubmitterIdentity {
			anonymous_id: "anon-42".to_owned(),
			email: Some("user@example.test".to_owned()),
		},
		app: RegisteredApp {
			slug: "notes-app".to_owned(),
		},
	}
}

#[test]
fn serializes_and_deserializes_round_trip() {
	let submission = sample_submission();
	let json = serde_json::to_string(&submission).unwrap();
	let parsed: FeedbackSubmission = serde_json::from_str(&json).unwrap();
	assert_eq!(parsed, submission);
}

#[test]
fn serializes_every_attachment_variant() {
	let variants = [
		Attachment::Screenshot {
			media_id: "shot-1".to_owned(),
		},
		Attachment::ScreenRecording {
			media_id: "clip-1".to_owned(),
		},
	];
	for attachment in variants {
		let mut submission = sample_submission();
		submission.attachments = vec![attachment.clone()];
		let json = serde_json::to_string(&submission).unwrap();
		let parsed: FeedbackSubmission = serde_json::from_str(&json).unwrap();
		assert_eq!(parsed.attachments, vec![attachment]);
	}
}
