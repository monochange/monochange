//! The feedback lifecycle state machine.
//!
//! Authority rules are encoded in the transitions rather than left to callers:
//! automated actors (`System`, `Ai`) may triage, ask questions, open voting,
//! and record build artifacts, but only `Maintainer` actors accept, decline,
//! or close. There is deliberately no merge or push command at all — releases
//! happen through the existing `GitHub` and monochange release flows, and the
//! pipeline only observes their outcome via [`Command::MarkShipped`].

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::submission::FeedbackSubmission;
use crate::triage::TriageReport;
use crate::voting::VoteTally;
use crate::voting::VotingOutcome;
use crate::voting::VotingRules;
use crate::voting::evaluate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
	Received,
	/// Paused because intake screening suspected untrusted instructions.
	Quarantined,
	Triaging,
	Discussing,
	Voting,
	Accepted,
	Declined,
	Building,
	InReview,
	Shipped,
	Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Actor {
	System,
	Ai,
	User(String),
	Maintainer(String),
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
	AskUser,
	OpenVoting,
	RecordVote { submitter_id: String },
	Accept(MaintainerDecision),
	Decline(MaintainerDecision),
	LinkIssue { number: u64, url: Option<String> },
	StartBuild,
	OpenPullRequest { number: u64, url: String },
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
			Command::RecordVote { .. } => "record-vote",
			Command::Accept(_) => "accept",
			Command::Decline(_) => "decline",
			Command::LinkIssue { .. } => "link-issue",
			Command::StartBuild => "start-build",
			Command::OpenPullRequest { .. } => "open-pull-request",
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
	#[error("submitter {0} has already voted")]
	DuplicateVote(String),
	#[error("accepting requires a triage report first")]
	MissingTriageReport,
	#[error("link a GitHub issue before starting the build")]
	IssueRequired,
	#[error("vote threshold not reached; provide an override rationale to accept anyway")]
	OverrideRequired,
}

/// Audit entry shown to maintainers as a timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackEvent {
	pub stage: Stage,
	pub actor: Actor,
	pub command: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackItem {
	pub submission: FeedbackSubmission,
	pub stage: Stage,
	pub rules: VotingRules,
	pub triage: Option<TriageReport>,
	pub votes: VoteTally,
	pub issue: Option<IssueRef>,
	pub pull_request: Option<PullRequestRef>,
	pub release: Option<ReleaseLink>,
	pub events: Vec<FeedbackEvent>,
}

impl FeedbackItem {
	pub fn new(submission: FeedbackSubmission, rules: VotingRules) -> Self {
		let mut item = Self {
			submission,
			stage: Stage::Received,
			rules,
			triage: None,
			votes: VoteTally::default(),
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
		match command {
			Command::Quarantine => {
				self.ensure(&[Stage::Received], name)?;
				self.enter(Stage::Quarantined, actor, name);
			}
			Command::StartTriage => {
				if self.stage == Stage::Quarantined && !matches!(actor, Actor::Maintainer(_)) {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				}
				self.ensure(&[Stage::Received, Stage::Quarantined], name)?;
				self.enter(Stage::Triaging, actor, name);
			}
			Command::CompleteTriage(report) => {
				self.ensure(&[Stage::Triaging], name)?;
				if !matches!(actor, Actor::System | Actor::Ai | Actor::Maintainer(_)) {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				}
				self.triage = Some(report);
				self.record_event(actor, name);
			}
			Command::AskUser => {
				self.ensure(&[Stage::Triaging], name)?;
				if !matches!(actor, Actor::System | Actor::Ai | Actor::Maintainer(_)) {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				}
				self.enter(Stage::Discussing, actor, name);
			}
			Command::OpenVoting => {
				self.ensure(&[Stage::Triaging, Stage::Discussing], name)?;
				if !matches!(actor, Actor::System | Actor::Ai | Actor::Maintainer(_)) {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				}
				self.enter(Stage::Voting, actor, name);
			}
			Command::RecordVote { submitter_id } => {
				self.ensure(&[Stage::Discussing, Stage::Voting], name)?;
				match actor {
					Actor::User(id) if id == &submitter_id => {}
					other => {
						return Err(TransitionError::ActorNotAllowed {
							command: name,
							actor: other.clone(),
						});
					}
				}
				if !self.votes.record(&submitter_id) {
					return Err(TransitionError::DuplicateVote(submitter_id));
				}
				self.record_event(actor, name);
			}
			Command::Accept(decision) => {
				self.ensure(&[Stage::Voting], name)?;
				let Actor::Maintainer(_) = actor else {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				};
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
				self.enter(Stage::Accepted, actor, name);
			}
			Command::Decline(_) => {
				self.ensure(&[Stage::Triaging, Stage::Discussing, Stage::Voting], name)?;
				let Actor::Maintainer(_) = actor else {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				};
				self.enter(Stage::Declined, actor, name);
			}
			Command::LinkIssue { number, url } => {
				self.ensure(&[Stage::Accepted, Stage::Building], name)?;
				self.issue = Some(IssueRef { number, url });
				self.record_event(actor, name);
			}
			Command::StartBuild => {
				self.ensure(&[Stage::Accepted], name)?;
				if self.issue.is_none() {
					return Err(TransitionError::IssueRequired);
				}
				self.enter(Stage::Building, actor, name);
			}
			Command::OpenPullRequest { number, url } => {
				self.ensure(&[Stage::Building], name)?;
				self.pull_request = Some(PullRequestRef { number, url });
				self.enter(Stage::InReview, actor, name);
			}
			Command::MarkShipped(release) => {
				self.ensure(&[Stage::InReview], name)?;
				self.release = Some(release);
				self.enter(Stage::Shipped, actor, name);
			}
			Command::Close => {
				self.ensure(
					&[
						Stage::Received,
						Stage::Quarantined,
						Stage::Triaging,
						Stage::Discussing,
						Stage::Voting,
						Stage::Accepted,
						Stage::Building,
						Stage::InReview,
					],
					name,
				)?;
				let Actor::Maintainer(_) = actor else {
					return Err(TransitionError::ActorNotAllowed {
						command: name,
						actor: actor.clone(),
					});
				};
				self.enter(Stage::Closed, actor, name);
			}
		}
		Ok(())
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
			command,
		});
	}
}

#[cfg(test)]
#[path = "__tests__/pipeline_tests.rs"]
mod tests;
