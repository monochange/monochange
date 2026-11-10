use monochange_app_feedback::Actor;
use monochange_app_feedback::DiscussionMessage;
use monochange_app_feedback::FeedbackItem;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::FeedbackSubmission;
use monochange_app_feedback::IssueRef;
use monochange_app_feedback::MaintainerDecision;
use monochange_app_feedback::PageContext;
use monochange_app_feedback::PublicStatus;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::ReproductionOutcome;
use monochange_app_feedback::Sensitivity;
use monochange_app_feedback::SimilarItem;
use monochange_app_feedback::Stage;
use monochange_app_feedback::SubmitterIdentity;
use monochange_app_feedback::TriageFinding;
use monochange_app_feedback::TriageReport;
use monochange_app_feedback::VotingRules;

use crate::server_fns::feedback::ConsoleAction;
use crate::server_fns::feedback::ConsoleItem;
use crate::server_fns::feedback::ConsoleView;

fn submission(route: &str) -> FeedbackSubmission {
	FeedbackSubmission {
		kind: FeedbackKind::BugReport,
		description: "The export button does nothing.".to_owned(),
		page: Some(PageContext {
			route: route.to_owned(),
			app_version: None,
			locale: None,
			element: None,
		}),
		attachments: Vec::new(),
		submitter: SubmitterIdentity {
			anonymous_id: "anon-1".to_owned(),
			email: None,
		},
		app_slug: "invoices".to_owned(),
	}
}

fn item(stage: Stage) -> FeedbackItem {
	let mut item = FeedbackItem::new(
		"fb-1".to_owned(),
		submission("/reports"),
		VotingRules::default(),
	);
	item.stage = stage;
	item.decision = Some(MaintainerDecision {
		maintainer: "ada".to_owned(),
		rationale: "It reproduces.".to_owned(),
		override_vote_threshold: None,
	});
	item.issue = Some(IssueRef {
		repository: Some("acme/invoices".to_owned()),
		number: 14,
		url: None,
	});
	item.triage = Some(TriageReport {
		classification: FeedbackKind::BugReport,
		reproduction: ReproductionOutcome::Reproduced {
			steps: vec!["Open reports.".to_owned()],
		},
		questions: Vec::new(),
		findings: vec![TriageFinding {
			detail: "a token".to_owned(),
			sensitivity: Sensitivity::Secret,
		}],
		product_summary: "Export does nothing".to_owned(),
	});
	item.discussion = vec![
		DiscussionMessage {
			author: Actor::Maintainer("ada".to_owned()),
			body: "Looking.".to_owned(),
			attachments: Vec::new(),
			held: true,
		},
		DiscussionMessage {
			author: Actor::Ai,
			body: "Tried it.".to_owned(),
			attachments: Vec::new(),
			held: true,
		},
		DiscussionMessage {
			author: Actor::System,
			body: "Held.".to_owned(),
			attachments: Vec::new(),
			held: true,
		},
		DiscussionMessage {
			author: Actor::User("anon-1".to_owned()),
			body: "Still broken.".to_owned(),
			attachments: Vec::new(),
			held: true,
		},
		DiscussionMessage {
			author: Actor::User("anon-2".to_owned()),
			body: "Same here.".to_owned(),
			attachments: Vec::new(),
			held: true,
		},
	];
	item
}

fn entry(stage: Stage, withheld: Option<&str>, title: Option<&str>, redacted: bool) -> ConsoleItem {
	ConsoleItem {
		item: item(stage),
		portal_title: title.map(str::to_owned),
		redacted: if redacted {
			vec!["/internal".to_owned()]
		} else {
			Vec::new()
		},
		withheld: withheld.map(str::to_owned),
		actions: vec![ConsoleAction::Resume],
		similar: vec![SimilarItem {
			id: "fb-2".to_owned(),
			title: "Export fails".to_owned(),
			status: PublicStatus::Planned,
			votes: 3,
			similarity_percent: 80,
		}],
		tasks: Vec::new(),
		shareable: false,
	}
}

#[test]
fn the_console_names_every_stage_author_and_public_repository() {
	for stage in [
		Stage::Received,
		Stage::Quarantined,
		Stage::Triaging,
		Stage::Discussing,
		Stage::Voting,
		Stage::Accepted,
		Stage::Declined,
		Stage::Building,
		Stage::InReview,
		Stage::Merged,
		Stage::Shipped,
		Stage::Closed,
	] {
		assert!(!super::stage_label(stage).is_empty());
	}

	let submitter = "anon-1";
	assert!(super::author_name(&Actor::Maintainer("ada".to_owned()), submitter).starts_with('@'));
	assert_eq!(super::author_name(&Actor::Ai, submitter), "Assistant");
	assert_eq!(super::author_name(&Actor::System, submitter), "System");
	assert!(super::author_name(&Actor::User("anon-1".to_owned()), submitter).contains("reporter"));
	assert_eq!(
		super::author_name(&Actor::User("anon-2".to_owned()), submitter),
		"anon-2"
	);

	let view = ConsoleView {
		organization: "acme".to_owned(),
		project_name: "Invoices".to_owned(),
		project_slug: "invoices".to_owned(),
		visibility: RepositoryVisibility::Public,
		acceptance_threshold: 5,
		shared_compute: false,
		repositories: vec!["acme/invoices".to_owned()],
		items: vec![
			entry(Stage::Building, Some("a secret"), None, false),
			entry(Stage::Accepted, None, Some("Export does nothing"), true),
			entry(Stage::Received, None, None, false),
		],
	};
	let lines = super::console_lines(&view);
	let text = lines.join("\n");
	assert!(
		text.contains("Every repository in this project is public"),
		"{text}"
	);
	assert!(text.contains("Route /reports"), "{text}");
	assert!(text.contains("Decided by @ada"), "{text}");
	assert!(!text.contains("threshold overridden"), "{text}");
	assert!(text.contains("An agent is building this"), "{text}");
	assert!(text.contains("Open reports."), "{text}");
	assert!(text.contains("Withheld from users"), "{text}");
	assert!(text.contains("Hidden:"), "{text}");
	assert!(text.contains("Not on the public roadmap"), "{text}");
	assert!(text.contains("@ada"), "{text}");
	assert!(text.contains("Assistant"), "{text}");
	assert!(text.contains("System"), "{text}");
	assert!(text.contains("Similar · fb-2"), "{text}");
}
