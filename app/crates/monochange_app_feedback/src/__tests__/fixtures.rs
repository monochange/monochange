//! Shared builders for the crate's unit tests.

use crate::CadenceSnapshot;
use crate::FeedbackService;
use crate::disclosure::DisclosurePolicy;
use crate::disclosure::RepositoryVisibility;
use crate::discussion::Actor;
use crate::pipeline::Command;
use crate::pipeline::FeedbackItem;
use crate::pipeline::MaintainerDecision;
use crate::pipeline::ReleaseLink;
use crate::pipeline::Stage;
use crate::submission::Attachment;
use crate::submission::FeedbackKind;
use crate::submission::FeedbackSubmission;
use crate::submission::PageContext;
use crate::submission::PinnedElement;
use crate::submission::RegisteredApp;
use crate::submission::SubmitterIdentity;
use crate::triage::ReproductionOutcome;
use crate::triage::RuleBasedTriage;
use crate::triage::TriageEngine;
use crate::triage::TriageReport;
use crate::voting::VotingRules;

pub const SUBMITTER: &str = "anon-1";
pub const APP: &str = "invoices";

pub fn screenshot(media_id: &str) -> Attachment {
	Attachment::Screenshot {
		media_id: media_id.to_owned(),
	}
}

pub fn page(route: &str, version: Option<&str>) -> PageContext {
	PageContext {
		route: route.to_owned(),
		app_version: version.map(str::to_owned),
		locale: None,
		element: None,
	}
}

pub fn pinned(route: &str, selector: &str, label: Option<&str>) -> PageContext {
	PageContext {
		element: Some(PinnedElement {
			selector: selector.to_owned(),
			label: label.map(str::to_owned),
		}),
		..page(route, Some("2.3.1"))
	}
}

/// A complete report: page, version, and a screenshot, so triage has no
/// questions for bugs.
pub fn submission(kind: FeedbackKind, description: &str) -> FeedbackSubmission {
	FeedbackSubmission {
		kind,
		description: description.to_owned(),
		page: Some(page("/reports/quarterly", Some("2.3.1"))),
		attachments: vec![screenshot("media-1")],
		submitter: SubmitterIdentity {
			anonymous_id: SUBMITTER.to_owned(),
			email: None,
		},
		app_slug: APP.to_owned(),
	}
}

/// A report with no page and no evidence.
pub fn bare_submission(kind: FeedbackKind, description: &str) -> FeedbackSubmission {
	FeedbackSubmission {
		page: None,
		attachments: Vec::new(),
		..submission(kind, description)
	}
}

pub fn from_submitter(mut submission: FeedbackSubmission, submitter: &str) -> FeedbackSubmission {
	submitter.clone_into(&mut submission.submitter.anonymous_id);
	submission
}

pub fn app() -> RegisteredApp {
	RegisteredApp {
		slug: APP.to_owned(),
		display_name: "Invoices".to_owned(),
		allowed_origins: vec!["https://invoices.example".to_owned()],
	}
}

pub fn service(visibility: RepositoryVisibility) -> FeedbackService<RuleBasedTriage> {
	let mut service = FeedbackService::new(
		RuleBasedTriage,
		DisclosurePolicy::for_visibility(visibility),
		VotingRules::default(),
		CadenceSnapshot {
			next_release_label: Some("v2.4 · around 21 October".to_owned()),
		},
	);
	service.register_app(app());
	service
}

pub fn decision(override_rationale: Option<&str>) -> MaintainerDecision {
	MaintainerDecision {
		maintainer: "ifiok".to_owned(),
		rationale: "Fits the roadmap".to_owned(),
		override_vote_threshold: override_rationale.map(str::to_owned),
	}
}

pub fn maintainer() -> Actor {
	Actor::Maintainer("ifiok".to_owned())
}

pub fn user(id: &str) -> Actor {
	Actor::User(id.to_owned())
}

pub fn report(classification: FeedbackKind, summary: &str) -> TriageReport {
	TriageReport {
		classification,
		reproduction: ReproductionOutcome::NotApplicable,
		questions: Vec::new(),
		findings: Vec::new(),
		product_summary: summary.to_owned(),
	}
}

/// Stages [`item_at`] can build, in pipeline order.
const PATH: [Stage; 9] = [
	Stage::Received,
	Stage::Triaging,
	Stage::Discussing,
	Stage::Voting,
	Stage::Accepted,
	Stage::Building,
	Stage::InReview,
	Stage::Merged,
	Stage::Shipped,
];

/// An item driven through real transitions to `stage`. Items past voting
/// were accepted with three votes, so no override was needed.
pub fn item_at(stage: Stage, submission: FeedbackSubmission) -> FeedbackItem {
	let mut item = FeedbackItem::new("fb-1".to_owned(), submission, VotingRules::default());
	let steps: [fn(&mut FeedbackItem); 8] = [
		|item| item.apply(Command::StartTriage, &Actor::System).unwrap(),
		|item| {
			let mut report = RuleBasedTriage.triage(&item.submission, &[]);
			if report.questions.is_empty() {
				report
					.questions
					.push("Which browser are you using?".to_owned());
			}
			item.apply(Command::CompleteTriage(report), &Actor::Ai)
				.unwrap();
			item.apply(Command::AskUser, &Actor::Ai).unwrap();
		},
		|item| item.apply(Command::OpenVoting, &Actor::System).unwrap(),
		|item| {
			for voter in ["v1", "v2", "v3"] {
				item.apply(Command::RecordVote, &user(voter)).unwrap();
			}
			item.apply(Command::Accept(decision(None)), &maintainer())
				.unwrap();
		},
		|item| {
			item.apply(
				Command::LinkIssue {
					number: 42,
					url: Some("https://github.com/acme/invoices/issues/42".to_owned()),
				},
				&Actor::System,
			)
			.unwrap();
			item.apply(Command::StartBuild, &Actor::System).unwrap();
		},
		|item| {
			item.apply(
				Command::OpenPullRequest {
					number: 43,
					url: "https://github.com/acme/invoices/pull/43".to_owned(),
				},
				&Actor::Ai,
			)
			.unwrap();
		},
		|item| item.apply(Command::MarkMerged, &Actor::System).unwrap(),
		|item| {
			item.apply(
				Command::MarkShipped(ReleaseLink {
					version: "2.4.0".to_owned(),
					notes_url: "https://invoices.example/releases/2.4.0".to_owned(),
				}),
				&Actor::System,
			)
			.unwrap();
		},
	];
	let target = PATH
		.iter()
		.position(|reachable| *reachable == stage)
		.unwrap_or_else(|| panic!("item_at cannot build {stage:?}"));
	for step in &steps[..target] {
		step(&mut item);
	}
	item
}
