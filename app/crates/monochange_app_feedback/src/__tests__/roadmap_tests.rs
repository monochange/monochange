use crate::disclosure::DisclosurePolicy;
use crate::disclosure::PublicLink;
use crate::disclosure::RepositoryVisibility;
use crate::pipeline::FeedbackItem;
use crate::pipeline::IssueRef;
use crate::pipeline::PullRequestRef;
use crate::pipeline::ReleaseLink;
use crate::pipeline::Stage;
use crate::roadmap::CadenceSnapshot;
use crate::roadmap::PublicStatus;
use crate::roadmap::RoadmapEntry;
use crate::roadmap::ShipWindow;
use crate::roadmap::public_item_update;
use crate::roadmap::public_status;
use crate::roadmap::public_status_line;
use crate::roadmap::public_title;
use crate::roadmap::roadmap_entry;
use crate::roadmap::ship_window;
use crate::roadmap::status_feed;
use crate::submission::Attachment;
use crate::submission::FeedbackKind;
use crate::submission::FeedbackSubmission;
use crate::submission::PageContext;
use crate::submission::RegisteredApp;
use crate::submission::SubmitterIdentity;
use crate::triage::ReproductionOutcome;
use crate::triage::RuleBasedTriage;
use crate::triage::TriageEngine;
use crate::triage::TriageReport;
use crate::voting::VotingRules;

fn bug_submission(id: &str) -> FeedbackSubmission {
	FeedbackSubmission {
		id: id.to_owned(),
		kind: FeedbackKind::BugReport,
		description: "Crashes when opening the quarterly view".to_owned(),
		page: Some(PageContext {
			route: "/reports/quarterly".to_owned(),
			app_version: Some("2.3.1".to_owned()),
			locale: None,
		}),
		attachments: vec![Attachment::Screenshot {
			media_id: "media-1".to_owned(),
		}],
		submitter: SubmitterIdentity {
			anonymous_id: "anon-1".to_owned(),
			email: None,
		},
		app: RegisteredApp {
			slug: "notes-app".to_owned(),
		},
	}
}

fn triaged_item(id: &str) -> FeedbackItem {
	let submission = bug_submission(id);
	let mut item = FeedbackItem::new(submission.clone(), VotingRules::default());
	let report = RuleBasedTriage.triage(&submission);
	item.triage = Some(report);
	item
}

#[test]
fn every_stage_maps_to_a_public_status() {
	let pairs = [
		(Stage::Received, PublicStatus::UnderReview),
		(Stage::Quarantined, PublicStatus::UnderReview),
		(Stage::Triaging, PublicStatus::UnderReview),
		(Stage::Discussing, PublicStatus::UnderReview),
		(Stage::Voting, PublicStatus::UnderReview),
		(Stage::Accepted, PublicStatus::Planned),
		(Stage::Building, PublicStatus::InProgress),
		(Stage::InReview, PublicStatus::InProgress),
		(Stage::Shipped, PublicStatus::Shipped),
		(Stage::Declined, PublicStatus::Declined),
		(Stage::Closed, PublicStatus::Declined),
	];
	for (stage, expected) in pairs {
		assert_eq!(public_status(&stage), expected, "stage {stage:?}");
		assert!(!public_status_line(&stage).is_empty());
	}
}

#[test]
fn ship_windows_follow_stage_and_cadence() {
	let cadence = CadenceSnapshot {
		next_window_label: Some("next scheduled release".to_owned()),
	};
	let mut item = triaged_item("fb-1");
	item.stage = Stage::Accepted;
	assert_eq!(
		ship_window(&item, &cadence),
		ShipWindow::Scheduled {
			label: "next scheduled release".to_owned(),
		}
	);
	item.stage = Stage::InReview;
	assert_eq!(
		ship_window(&item, &CadenceSnapshot::default()),
		ShipWindow::Unscheduled
	);
	item.stage = Stage::Voting;
	assert_eq!(ship_window(&item, &cadence), ShipWindow::Unscheduled);
	item.stage = Stage::Shipped;
	item.release = Some(ReleaseLink {
		version: "2.4.0".to_owned(),
		notes_url: "https://notes.dev/releases/2.4.0.json".to_owned(),
	});
	assert_eq!(
		ship_window(&item, &cadence),
		ShipWindow::Scheduled {
			label: "2.4.0".to_owned(),
		}
	);
}

#[test]
fn shipped_without_release_is_unscheduled() {
	let mut item = triaged_item("fb-1");
	item.stage = Stage::Shipped;
	assert_eq!(
		ship_window(&item, &CadenceSnapshot::default()),
		ShipWindow::Unscheduled
	);
}

#[test]
fn roadmap_entry_gates_links_by_policy() {
	let mut item = triaged_item("fb-1");
	item.stage = Stage::InReview;
	item.votes.record("u1");
	item.votes.record("u2");
	item.issue = Some(IssueRef {
		number: 1,
		url: Some("https://github.com/acme/notes/issues/1".to_owned()),
	});
	item.pull_request = Some(PullRequestRef {
		number: 2,
		url: "https://github.com/acme/notes/pull/2".to_owned(),
	});
	let cadence = CadenceSnapshot::default();

	let public = DisclosurePolicy::for_visibility(RepositoryVisibility::Public);
	let entry = roadmap_entry(&item, public, &cadence).unwrap();
	assert_eq!(entry.status, PublicStatus::InProgress);
	assert_eq!(entry.votes, 2);
	assert_eq!(
		entry.links,
		vec![
			PublicLink::Issue("https://github.com/acme/notes/issues/1".to_owned()),
			PublicLink::PullRequest("https://github.com/acme/notes/pull/2".to_owned()),
		]
	);

	let private = DisclosurePolicy::for_visibility(RepositoryVisibility::Private);
	let private_entry = roadmap_entry(&item, private, &cadence).unwrap();
	assert!(private_entry.links.is_empty());
}

#[test]
fn status_feed_splits_shipped_from_active() {
	let mut voting_item = triaged_item("fb-1");
	voting_item.stage = Stage::Voting;

	let mut shipped_item = triaged_item("fb-2");
	shipped_item.stage = Stage::Shipped;
	shipped_item.release = Some(ReleaseLink {
		version: "2.4.0".to_owned(),
		notes_url: "https://notes.dev/releases/2.4.0.json".to_owned(),
	});

	let mut declined_item = triaged_item("fb-3");
	declined_item.stage = Stage::Declined;

	let items = [voting_item, shipped_item, declined_item];
	let policy = DisclosurePolicy::for_visibility(RepositoryVisibility::Public);
	let cadence = CadenceSnapshot::default();
	let feed = status_feed(items.iter(), policy, &cadence).unwrap();
	assert_eq!(feed.roadmap.len(), 2);
	assert_eq!(feed.roadmap[0].status, PublicStatus::UnderReview);
	assert_eq!(feed.roadmap[1].status, PublicStatus::Declined);
	assert_eq!(feed.shipped.len(), 1);
	assert_eq!(feed.shipped[0].version, "2.4.0");
	assert_eq!(
		feed.shipped[0].notes_url,
		"https://notes.dev/releases/2.4.0.json"
	);
}

#[test]
fn public_title_prefers_triage_summary_and_truncates() {
	let mut item = triaged_item("fb-1");
	let Some(report) = item.triage.as_mut() else {
		panic!("triage expected");
	};
	report.product_summary = "x".repeat(140);
	assert_eq!(public_title(&item).chars().count(), 96);

	let mut untriaged = FeedbackItem::new(bug_submission("fb-2"), VotingRules::default());
	untriaged.submission.description = "Add offline mode\nThe rest is detail".to_owned();
	assert_eq!(public_title(&untriaged), "Add offline mode");
}

#[test]
fn public_item_update_redacts_for_private_repositories() {
	let mut item = triaged_item("fb-1");
	let Some(report) = item.triage.as_mut() else {
		panic!("triage expected");
	};
	report.product_summary =
		"Reported a problem on /billing: totals wrong inside /app/src/billing.rs".to_owned();
	item.stage = Stage::Building;
	item.issue = Some(IssueRef {
		number: 1,
		url: Some("https://github.com/acme/notes/issues/1".to_owned()),
	});
	let private = DisclosurePolicy::for_visibility(RepositoryVisibility::Private);
	let update = public_item_update(&item, private).unwrap();
	assert!(update.title.contains("[redacted]"));
	assert!(!update.title.contains("billing.rs"));
	assert!(update.links.is_empty());
	assert_eq!(update.technical_detail, None);
}

#[test]
fn manual_triage_reports_can_drive_entries() {
	let submission = bug_submission("fb-1");
	let mut item = FeedbackItem::new(submission, VotingRules::default());
	item.triage = Some(TriageReport {
		classification: FeedbackKind::FeatureRequest,
		reproduction: ReproductionOutcome::NotApplicable,
		questions: Vec::new(),
		findings: Vec::new(),
		product_summary: "Requested: dark mode".to_owned(),
	});
	item.stage = Stage::Voting;
	let policy = DisclosurePolicy::for_visibility(RepositoryVisibility::Public);
	let cadence = CadenceSnapshot::default();
	let entry = roadmap_entry(&item, policy, &cadence).unwrap();
	assert_eq!(entry.title, "Requested: dark mode");
}

#[test]
fn wire_types_round_trip_through_clone_and_serialization() {
	let mut item = triaged_item("fb-1");
	item.stage = Stage::Voting;
	let policy = DisclosurePolicy::for_visibility(RepositoryVisibility::Public);
	let cadence = CadenceSnapshot {
		next_window_label: Some("next scheduled release".to_owned()),
	};
	let entry = roadmap_entry(&item, policy, &cadence).unwrap();
	let cloned_entry = entry.clone();
	assert_eq!(cloned_entry, entry);
	assert_eq!(cadence.clone(), cadence);
	let serialized = serde_json::to_string(&entry).unwrap();
	assert!(serialized.contains("\"status\":\"UnderReview\""));
}

#[test]
fn roadmap_entries_survive_serialization_round_trip() {
	let mut item = triaged_item("fb-1");
	item.stage = Stage::Voting;
	let policy = DisclosurePolicy::for_visibility(RepositoryVisibility::Public);
	let cadence = CadenceSnapshot::default();
	let entry = roadmap_entry(&item, policy, &cadence).unwrap();
	let serialized = serde_json::to_string(&entry).unwrap();
	let round_trip: RoadmapEntry = serde_json::from_str(&serialized).unwrap();
	assert_eq!(round_trip, entry);
}
