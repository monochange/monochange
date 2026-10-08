use crate::pipeline::Actor;
use crate::pipeline::Command;
use crate::pipeline::FeedbackItem;
use crate::pipeline::MaintainerDecision;
use crate::pipeline::ReleaseLink;
use crate::pipeline::Stage;
use crate::pipeline::TransitionError;
use crate::submission::Attachment;
use crate::submission::FeedbackKind;
use crate::submission::FeedbackSubmission;
use crate::submission::PageContext;
use crate::submission::RegisteredApp;
use crate::submission::SubmitterIdentity;
use crate::triage::RuleBasedTriage;
use crate::triage::TriageEngine;
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

fn decision(maintainer: &str) -> MaintainerDecision {
	MaintainerDecision {
		maintainer: maintainer.to_owned(),
		rationale: "aligns with the roadmap".to_owned(),
		override_vote_threshold: None,
	}
}

fn override_decision(maintainer: &str) -> MaintainerDecision {
	MaintainerDecision {
		override_vote_threshold: Some("first-party priority".to_owned()),
		..decision(maintainer)
	}
}

fn item_at_voting(id: &str) -> FeedbackItem {
	let submission = bug_submission(id);
	let mut item = FeedbackItem::new(submission.clone(), VotingRules::default());
	item.apply(Command::StartTriage, &Actor::System).unwrap();
	let report = RuleBasedTriage.triage(&submission);
	item.apply(Command::CompleteTriage(report), &Actor::Ai)
		.unwrap();
	item.apply(Command::OpenVoting, &Actor::System).unwrap();
	item
}

fn cast_votes(item: &mut FeedbackItem, submitters: &[&str]) {
	for submitter in submitters {
		item.apply(
			Command::RecordVote {
				submitter_id: (*submitter).to_owned(),
			},
			&Actor::User((*submitter).to_owned()),
		)
		.unwrap();
	}
}

#[test]
fn new_items_start_received_with_audit_event() {
	let item = FeedbackItem::new(bug_submission("fb-1"), VotingRules::default());
	assert_eq!(item.stage, Stage::Received);
	assert_eq!(item.events.len(), 1);
	assert_eq!(item.events[0].command, "receive");
}

#[test]
fn happy_path_reaches_shipped() {
	let mut item = item_at_voting("fb-1");
	cast_votes(&mut item, &["u1", "u2", "u3"]);
	item.apply(
		Command::Accept(decision("ifiok")),
		&Actor::Maintainer("ifiok".to_owned()),
	)
	.unwrap();
	assert_eq!(item.stage, Stage::Accepted);
	item.apply(
		Command::LinkIssue {
			number: 1,
			url: Some("https://github.com/acme/notes/issues/1".to_owned()),
		},
		&Actor::System,
	)
	.unwrap();
	item.apply(Command::StartBuild, &Actor::System).unwrap();
	assert_eq!(item.stage, Stage::Building);
	item.apply(
		Command::OpenPullRequest {
			number: 2,
			url: "https://github.com/acme/notes/pull/2".to_owned(),
		},
		&Actor::System,
	)
	.unwrap();
	assert_eq!(item.stage, Stage::InReview);
	item.apply(
		Command::MarkShipped(ReleaseLink {
			version: "2.4.0".to_owned(),
			notes_url: "https://notes.dev/releases/2.4.0.json".to_owned(),
		}),
		&Actor::System,
	)
	.unwrap();
	assert_eq!(item.stage, Stage::Shipped);
	assert_eq!(item.issue.as_ref().unwrap().number, 1);
	assert_eq!(item.pull_request.as_ref().unwrap().number, 2);
	assert_eq!(item.release.as_ref().unwrap().version, "2.4.0");
	assert_eq!(item.events.last().unwrap().command, "mark-shipped");
}

#[test]
fn accept_requires_threshold_or_override() {
	let mut item = item_at_voting("fb-1");
	cast_votes(&mut item, &["u1"]);
	let error = item
		.apply(
			Command::Accept(decision("ifiok")),
			&Actor::Maintainer("ifiok".to_owned()),
		)
		.unwrap_err();
	assert_eq!(error, TransitionError::OverrideRequired);
	item.apply(
		Command::Accept(override_decision("ifiok")),
		&Actor::Maintainer("ifiok".to_owned()),
	)
	.unwrap();
	assert_eq!(item.stage, Stage::Accepted);
}

#[test]
fn only_maintainers_accept() {
	let mut item = item_at_voting("fb-1");
	cast_votes(&mut item, &["u1", "u2", "u3"]);
	let error = item
		.apply(Command::Accept(decision("ifiok")), &Actor::System)
		.unwrap_err();
	assert!(matches!(
		error,
		TransitionError::ActorNotAllowed {
			command: "accept",
			actor: Actor::System,
		}
	));
}

#[test]
fn votes_dedupe_and_validate_actor() {
	let mut item = item_at_voting("fb-1");
	cast_votes(&mut item, &["u1"]);
	let duplicate = item
		.apply(
			Command::RecordVote {
				submitter_id: "u1".to_owned(),
			},
			&Actor::User("u1".to_owned()),
		)
		.unwrap_err();
	assert_eq!(duplicate, TransitionError::DuplicateVote("u1".to_owned()));
	let forged = item
		.apply(
			Command::RecordVote {
				submitter_id: "u2".to_owned(),
			},
			&Actor::Ai,
		)
		.unwrap_err();
	assert!(matches!(
		forged,
		TransitionError::ActorNotAllowed {
			command: "record-vote",
			actor: Actor::Ai,
		}
	));
}

#[test]
fn build_requires_linked_issue() {
	let mut item = item_at_voting("fb-1");
	cast_votes(&mut item, &["u1", "u2", "u3"]);
	item.apply(
		Command::Accept(decision("ifiok")),
		&Actor::Maintainer("ifiok".to_owned()),
	)
	.unwrap();
	let error = item.apply(Command::StartBuild, &Actor::System).unwrap_err();
	assert_eq!(error, TransitionError::IssueRequired);
	item.apply(
		Command::LinkIssue {
			number: 1,
			url: None,
		},
		&Actor::System,
	)
	.unwrap();
	item.apply(Command::StartBuild, &Actor::System).unwrap();
	assert_eq!(item.stage, Stage::Building);
}

#[test]
fn illegal_transitions_are_rejected() {
	let mut from_received = FeedbackItem::new(bug_submission("fb-2"), VotingRules::default());
	assert_eq!(
		from_received
			.apply(Command::OpenVoting, &Actor::System)
			.unwrap_err(),
		TransitionError::Illegal {
			command: "open-voting",
			from: Stage::Received,
		}
	);
	let mut item = item_at_voting("fb-1");
	let from_voting = item
		.apply(
			Command::MarkShipped(ReleaseLink {
				version: "2.4.0".to_owned(),
				notes_url: "https://notes.dev/releases/2.4.0.json".to_owned(),
			}),
			&Actor::System,
		)
		.unwrap_err();
	assert_eq!(
		from_voting,
		TransitionError::Illegal {
			command: "mark-shipped",
			from: Stage::Voting,
		}
	);
}

#[test]
fn quarantine_requires_maintainer_to_resume() {
	let mut item = FeedbackItem::new(bug_submission("fb-1"), VotingRules::default());
	item.apply(Command::Quarantine, &Actor::System).unwrap();
	assert_eq!(item.stage, Stage::Quarantined);
	let system = item
		.apply(Command::StartTriage, &Actor::System)
		.unwrap_err();
	assert!(matches!(
		system,
		TransitionError::ActorNotAllowed {
			command: "start-triage",
			actor: Actor::System,
		}
	));
	item.apply(Command::StartTriage, &Actor::Maintainer("ifiok".to_owned()))
		.unwrap();
	assert_eq!(item.stage, Stage::Triaging);
	let double_quarantine = item.apply(Command::Quarantine, &Actor::System).unwrap_err();
	assert!(matches!(
		double_quarantine,
		TransitionError::Illegal {
			command: "quarantine",
			from: Stage::Triaging,
		}
	));
}

#[test]
fn accept_needs_triage_report() {
	let submission = bug_submission("fb-1");
	let mut item = FeedbackItem::new(submission, VotingRules::default());
	item.apply(Command::StartTriage, &Actor::System).unwrap();
	item.apply(Command::OpenVoting, &Actor::System).unwrap();
	let error = item
		.apply(
			Command::Accept(override_decision("ifiok")),
			&Actor::Maintainer("ifiok".to_owned()),
		)
		.unwrap_err();
	assert_eq!(error, TransitionError::MissingTriageReport);
}

#[test]
fn maintainer_closes_open_items() {
	let mut item = item_at_voting("fb-1");
	let denied = item
		.apply(Command::Close, &Actor::User("u1".to_owned()))
		.unwrap_err();
	assert!(matches!(
		denied,
		TransitionError::ActorNotAllowed {
			command: "close",
			actor: Actor::User(_),
		}
	));
	item.apply(Command::Close, &Actor::Maintainer("ifiok".to_owned()))
		.unwrap();
	assert_eq!(item.stage, Stage::Closed);
}

#[test]
fn decline_from_voting() {
	let mut item = item_at_voting("fb-1");
	item.apply(
		Command::Decline(decision("ifiok")),
		&Actor::Maintainer("ifiok".to_owned()),
	)
	.unwrap();
	assert_eq!(item.stage, Stage::Declined);
}

#[test]
fn automated_commands_reject_user_actors() {
	let submission = bug_submission("fb-1");
	let mut item = FeedbackItem::new(submission.clone(), VotingRules::default());
	item.apply(Command::StartTriage, &Actor::System).unwrap();
	let commands = [
		Command::CompleteTriage(RuleBasedTriage.triage(&submission)),
		Command::AskUser,
		Command::OpenVoting,
	];
	for command in commands {
		let error = item
			.apply(command, &Actor::User("u1".to_owned()))
			.unwrap_err();
		assert!(matches!(error, TransitionError::ActorNotAllowed { .. }));
	}
	assert_eq!(item.stage, Stage::Triaging);

	item.apply(Command::OpenVoting, &Actor::System).unwrap();
	let declined_by_system = item
		.apply(Command::Decline(decision("ifiok")), &Actor::System)
		.unwrap_err();
	assert!(matches!(
		declined_by_system,
		TransitionError::ActorNotAllowed {
			command: "decline",
			actor: Actor::System,
		}
	));
	let closed_by_ai = item.apply(Command::Close, &Actor::Ai).unwrap_err();
	assert!(matches!(
		closed_by_ai,
		TransitionError::ActorNotAllowed {
			command: "close",
			actor: Actor::Ai,
		}
	));
}

#[test]
fn terminal_stages_cannot_be_closed() {
	let mut item = FeedbackItem::new(bug_submission("fb-1"), VotingRules::default());
	item.stage = Stage::Shipped;
	let error = item
		.apply(Command::Close, &Actor::Maintainer("ifiok".to_owned()))
		.unwrap_err();
	assert_eq!(
		error,
		TransitionError::Illegal {
			command: "close",
			from: Stage::Shipped,
		}
	);
}
