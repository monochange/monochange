//! The feedback lifecycle state machine.
//!
//! ```text
//! Received ─┬─> Triaging ─┬─> Discussing ─┬─> Voting ─> Accepted ─> Building ─> InReview ─> Merged ─> Shipped
//!           │      ^      │               │     │
//!           └─> Quarantined (maintainer resumes)  └──────── Declined / Closed (maintainer) ───────┘
//! ```
//!
//! Authority rules are encoded in the transitions rather than left to callers:
//! automated actors (`System`, `Ai`) may triage, ask questions, open voting,
//! and record build artifacts; users may discuss and vote; only maintainers
//! accept, decline, merge duplicates, rewrite public summaries, or close.
//! There is deliberately no merge or push command — code lands through the
//! repository's own review, and the pipeline only observes the outcome via
//! [`Command::MarkMerged`] and [`Command::MarkShipped`].

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::discussion::Actor;
use crate::discussion::DiscussionMessage;
use crate::submission::Attachment;
use crate::submission::FeedbackSubmission;
use crate::triage::ScreeningVerdict;
use crate::triage::TriageReport;
use crate::triage::screen_untrusted;
use crate::voting::VoteTally;
use crate::voting::VotingOutcome;
use crate::voting::VotingRules;
use crate::voting::evaluate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
	Received,
	/// Paused because intake screening suspected untrusted instructions.
	Quarantined,
	Triaging,
	/// Triage asked follow-up questions; the submitter's reply re-runs it.
	Discussing,
	Voting,
	Accepted,
	Declined,
	Building,
	InReview,
	/// The pull request merged; the change ships with the next release.
	Merged,
	Shipped,
	Closed,
}

impl Stage {
	/// Whether the item can still change. Declined, shipped, and closed items
	/// are final.
	pub fn is_open(self) -> bool {
		!matches!(self, Stage::Declined | Stage::Shipped | Stage::Closed)
	}
}

/// A maintainer decision on an item. When the vote threshold has not been
/// reached, accepting requires an explicit override rationale so bypassing
/// community demand is always a recorded, reviewable act.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintainerDecision {
	pub maintainer: String,
	pub rationale: String,
	pub override_vote_threshold: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueRef {
	pub number: u64,
	pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestRef {
	pub number: u64,
	pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseLink {
	pub version: String,
	pub notes_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
	Quarantine,
	StartTriage,
	CompleteTriage(TriageReport),
	/// Posts the triage report's questions to the thread.
	AskUser,
	OpenVoting,
	PostMessage {
		body: String,
		attachments: Vec<Attachment>,
	},
	/// The voter is the acting [`Actor::User`].
	RecordVote,
	RetractVote,
	/// Replaces the public title. Maintainers use this to rewrite a neutral
	/// or leaky summary before it reaches the roadmap.
	EditSummary(String),
	Accept(MaintainerDecision),
	Decline(MaintainerDecision),
	MarkDuplicate {
		of: String,
	},
	LinkIssue {
		number: u64,
		url: Option<String>,
	},
	StartBuild,
	OpenPullRequest {
		number: u64,
		url: String,
	},
	MarkMerged,
	MarkShipped(ReleaseLink),
	Close,
}

impl Command {
	pub fn name(&self) -> &'static str {
		match self {
			Command::Quarantine => "quarantine",
			Command::StartTriage => "start-triage",
			Command::CompleteTriage(_) => "complete-triage",
			Command::AskUser => "ask-user",
			Command::OpenVoting => "open-voting",
			Command::PostMessage { .. } => "post-message",
			Command::RecordVote => "record-vote",
			Command::RetractVote => "retract-vote",
			Command::EditSummary(_) => "edit-summary",
			Command::Accept(_) => "accept",
			Command::Decline(_) => "decline",
			Command::MarkDuplicate { .. } => "mark-duplicate",
			Command::LinkIssue { .. } => "link-issue",
			Command::StartBuild => "start-build",
			Command::OpenPullRequest { .. } => "open-pull-request",
			Command::MarkMerged => "mark-merged",
			Command::MarkShipped(_) => "mark-shipped",
			Command::Close => "close",
		}
	}
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TransitionError {
	#[error("{command} is not allowed while the item is {from:?}")]
	Illegal { command: &'static str, from: Stage },
	#[error("{command} is not allowed for actor {actor:?}")]
	ActorNotAllowed { command: &'static str, actor: Actor },
	#[error("{0} has already voted")]
	DuplicateVote(String),
	#[error("{0} has not voted")]
	NotVoted(String),
	#[error("this step requires a triage report first")]
	MissingTriageReport,
	#[error("link a GitHub issue before starting the build")]
	IssueRequired,
	#[error("vote threshold not reached; provide an override rationale to accept anyway")]
	OverrideRequired,
	#[error("an item cannot be a duplicate of itself")]
	SelfDuplicate,
	#[error("the text is empty")]
	EmptyText,
}

/// Audit entry shown to maintainers as a timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEvent {
	pub stage: Stage,
	pub actor: Actor,
	pub command: String,
}

const ANY_OPEN: [Stage; 9] = [
	Stage::Received,
	Stage::Quarantined,
	Stage::Triaging,
	Stage::Discussing,
	Stage::Voting,
	Stage::Accepted,
	Stage::Building,
	Stage::InReview,
	Stage::Merged,
];
const BEFORE_DECISION: [Stage; 4] = [
	Stage::Quarantined,
	Stage::Triaging,
	Stage::Discussing,
	Stage::Voting,
];
const DISCUSSABLE: [Stage; 6] = [
	Stage::Discussing,
	Stage::Voting,
	Stage::Accepted,
	Stage::Building,
	Stage::InReview,
	Stage::Merged,
];
const VOTABLE: [Stage; 5] = [
	Stage::Discussing,
	Stage::Voting,
	Stage::Accepted,
	Stage::Building,
	Stage::InReview,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackItem {
	pub id: String,
	pub submission: FeedbackSubmission,
	pub stage: Stage,
	pub rules: VotingRules,
	pub triage: Option<TriageReport>,
	/// Maintainer-written public title; wins over the triage summary.
	pub summary_override: Option<String>,
	pub votes: VoteTally,
	pub discussion: Vec<DiscussionMessage>,
	/// Anonymous ids notified when the item's public status changes: the
	/// submitter, voters, and users who joined the discussion.
	pub subscribers: BTreeSet<String>,
	pub decision: Option<MaintainerDecision>,
	pub duplicate_of: Option<String>,
	pub issue: Option<IssueRef>,
	pub pull_request: Option<PullRequestRef>,
	pub release: Option<ReleaseLink>,
	pub events: Vec<FeedbackEvent>,
}

impl FeedbackItem {
	pub fn new(id: String, submission: FeedbackSubmission, rules: VotingRules) -> Self {
		let subscribers = BTreeSet::from([submission.submitter.anonymous_id.clone()]);
		let mut item = Self {
			id,
			submission,
			stage: Stage::Received,
			rules,
			triage: None,
			summary_override: None,
			votes: VoteTally::default(),
			discussion: Vec::new(),
			subscribers,
			decision: None,
			duplicate_of: None,
			issue: None,
			pull_request: None,
			release: None,
			events: Vec::new(),
		};
		item.record_event(&Actor::System, "receive");
		item
	}

	/// Applies a command, enforcing stage order and actor authority.
	pub fn apply(&mut self, command: Command, actor: &Actor) -> Result<(), TransitionError> {
		let name = command.name();
		let automation_or_maintainer = actor.is_automation() || actor.is_maintainer();
		match command {
			Command::Quarantine => {
				self.ensure(&[Stage::Received], name)?;
				authorize(name, actor, actor.is_automation())?;
				self.enter(Stage::Quarantined, actor, name);
			}
			Command::StartTriage => {
				self.ensure(&[Stage::Received, Stage::Quarantined], name)?;
				// Only a maintainer can vouch for quarantined text.
				let allowed = if self.stage == Stage::Quarantined {
					actor.is_maintainer()
				} else {
					automation_or_maintainer
				};
				authorize(name, actor, allowed)?;
				self.enter(Stage::Triaging, actor, name);
			}
			Command::CompleteTriage(report) => {
				self.ensure(&[Stage::Triaging, Stage::Discussing], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				self.triage = Some(report);
				self.record_event(actor, name);
			}
			Command::AskUser => {
				self.ensure(&[Stage::Triaging], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				let questions = self
					.triage
					.as_ref()
					.ok_or(TransitionError::MissingTriageReport)?
					.questions
					.join("\n");
				if questions.trim().is_empty() {
					return Err(TransitionError::EmptyText);
				}
				self.discussion.push(DiscussionMessage {
					author: actor.clone(),
					body: questions,
					attachments: Vec::new(),
					held: false,
				});
				self.enter(Stage::Discussing, actor, name);
			}
			Command::OpenVoting => {
				self.ensure(&[Stage::Triaging, Stage::Discussing], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				self.enter(Stage::Voting, actor, name);
			}
			Command::PostMessage { body, attachments } => {
				self.ensure(&DISCUSSABLE, name)?;
				authorize(name, actor, *actor != Actor::System)?;
				if body.trim().is_empty() && attachments.is_empty() {
					return Err(TransitionError::EmptyText);
				}
				let held = matches!(actor, Actor::User(_))
					&& matches!(
						screen_untrusted(&body),
						ScreeningVerdict::InjectionSuspected { .. }
					);
				if let (Actor::User(id), false) = (actor, held) {
					self.subscribers.insert(id.clone());
				}
				self.discussion.push(DiscussionMessage {
					author: actor.clone(),
					body,
					attachments,
					held,
				});
				self.record_event(actor, name);
			}
			Command::RecordVote => {
				self.ensure(&VOTABLE, name)?;
				let voter = user_id(name, actor)?;
				if !self.votes.record(voter) {
					return Err(TransitionError::DuplicateVote(voter.to_owned()));
				}
				self.subscribers.insert(voter.to_owned());
				self.record_event(actor, name);
			}
			Command::RetractVote => {
				self.ensure(&VOTABLE, name)?;
				let voter = user_id(name, actor)?;
				if !self.votes.retract(voter) {
					return Err(TransitionError::NotVoted(voter.to_owned()));
				}
				self.record_event(actor, name);
			}
			Command::EditSummary(summary) => {
				self.ensure(&ANY_OPEN[1..], name)?;
				authorize(name, actor, actor.is_maintainer())?;
				let summary = summary.trim();
				if summary.is_empty() {
					return Err(TransitionError::EmptyText);
				}
				self.summary_override = Some(summary.to_owned());
				self.record_event(actor, name);
			}
			Command::Accept(decision) => {
				self.ensure(&[Stage::Discussing, Stage::Voting], name)?;
				authorize(name, actor, actor.is_maintainer())?;
				if self.triage.is_none() {
					return Err(TransitionError::MissingTriageReport);
				}
				let threshold_reached = matches!(
					evaluate(&self.votes, &self.rules),
					VotingOutcome::ThresholdReached { .. }
				);
				if !threshold_reached && decision.override_vote_threshold.is_none() {
					return Err(TransitionError::OverrideRequired);
				}
				self.decision = Some(decision);
				self.enter(Stage::Accepted, actor, name);
			}
			Command::Decline(decision) => {
				self.ensure(&BEFORE_DECISION, name)?;
				authorize(name, actor, actor.is_maintainer())?;
				self.decision = Some(decision);
				self.enter(Stage::Declined, actor, name);
			}
			Command::MarkDuplicate { of } => {
				self.ensure(&BEFORE_DECISION, name)?;
				authorize(name, actor, actor.is_maintainer())?;
				if of == self.id {
					return Err(TransitionError::SelfDuplicate);
				}
				self.duplicate_of = Some(of);
				self.enter(Stage::Closed, actor, name);
			}
			Command::LinkIssue { number, url } => {
				self.ensure(&[Stage::Accepted, Stage::Building], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				self.issue = Some(IssueRef { number, url });
				self.record_event(actor, name);
			}
			Command::StartBuild => {
				self.ensure(&[Stage::Accepted], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				if self.issue.is_none() {
					return Err(TransitionError::IssueRequired);
				}
				self.enter(Stage::Building, actor, name);
			}
			Command::OpenPullRequest { number, url } => {
				self.ensure(&[Stage::Building], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				self.pull_request = Some(PullRequestRef { number, url });
				self.enter(Stage::InReview, actor, name);
			}
			Command::MarkMerged => {
				self.ensure(&[Stage::InReview], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				self.enter(Stage::Merged, actor, name);
			}
			Command::MarkShipped(release) => {
				// A release that contains the pull request implies the merge,
				// so shipping straight from review is legal.
				self.ensure(&[Stage::InReview, Stage::Merged], name)?;
				authorize(name, actor, automation_or_maintainer)?;
				self.release = Some(release);
				self.enter(Stage::Shipped, actor, name);
			}
			Command::Close => {
				self.ensure(&ANY_OPEN, name)?;
				authorize(name, actor, actor.is_maintainer())?;
				self.enter(Stage::Closed, actor, name);
			}
		}
		Ok(())
	}

	/// Messages that triage and other users may see.
	pub fn visible_discussion(&self) -> Vec<&DiscussionMessage> {
		crate::discussion::visible_messages(&self.discussion).collect()
	}

	/// Folds a duplicate's demand into this item: its submitter and voters
	/// count as votes here and keep receiving updates. The duplicate itself
	/// moves with [`Command::MarkDuplicate`]; this is the canonical side.
	pub fn absorb_duplicate(&mut self, duplicate: &FeedbackItem, maintainer: &Actor) {
		self.votes
			.record(&duplicate.submission.submitter.anonymous_id);
		for voter in duplicate.votes.voters() {
			self.votes.record(voter);
		}
		self.subscribers
			.extend(duplicate.subscribers.iter().cloned());
		self.record_event(maintainer, "absorb-duplicate");
	}

	fn ensure(&self, allowed: &[Stage], command: &'static str) -> Result<(), TransitionError> {
		if allowed.contains(&self.stage) {
			Ok(())
		} else {
			Err(TransitionError::Illegal {
				command,
				from: self.stage,
			})
		}
	}

	fn enter(&mut self, stage: Stage, actor: &Actor, command: &'static str) {
		self.stage = stage;
		self.record_event(actor, command);
	}

	fn record_event(&mut self, actor: &Actor, command: &'static str) {
		self.events.push(FeedbackEvent {
			stage: self.stage,
			actor: actor.clone(),
			command: command.to_owned(),
		});
	}
}

fn authorize(command: &'static str, actor: &Actor, allowed: bool) -> Result<(), TransitionError> {
	if allowed {
		Ok(())
	} else {
		Err(TransitionError::ActorNotAllowed {
			command,
			actor: actor.clone(),
		})
	}
}

fn user_id<'a>(command: &'static str, actor: &'a Actor) -> Result<&'a str, TransitionError> {
	match actor {
		Actor::User(id) => Ok(id),
		other => {
			Err(TransitionError::ActorNotAllowed {
				command,
				actor: other.clone(),
			})
		}
	}
}

#[cfg(test)]
#[path = "__tests__/pipeline_tests.rs"]
mod tests;
