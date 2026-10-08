//! Feedback pipeline for the monochange web app.
//!
//! This crate models the loop between users of third-party apps and
//! maintainers:
//!
//! 1. **Intake** — users submit bug reports or feature requests with a
//!    description, optional page context, and optional screenshots
//!    ([`FeedbackSubmission`]).
//! 2. **Screening** — untrusted text is checked for prompt-injection markers
//!    before any automated processing; suspect items are quarantined until a
//!    maintainer approves them ([`screen_untrusted`]).
//! 3. **Triage** — a [`TriageEngine`] classifies the report, attempts a
//!    reproduction, and produces follow-up questions plus maintainer-only
//!    technical findings ([`TriageReport`]). The rule-based engine here stands
//!    in for the AI agent.
//! 4. **Discussion and voting** — users answer questions and vote; votes
//!    signal demand but never accept work on their own.
//! 5. **Decision** — only maintainers accept or decline; accepting below the
//!    vote threshold requires a recorded override rationale
//!    ([`MaintainerDecision`]).
//! 6. **Build handoff** — accepted items link a `GitHub` issue and pull
//!    request. The pipeline records progress but deliberately has no merge or
//!    push command; releases happen through the existing monochange release
//!    flow and are observed via [`Command::MarkShipped`].
//! 7. **Status feedback** — [`DisclosureGate`] renders every user-facing
//!    update under the repository's [`DisclosurePolicy`], so private
//!    codebases never leak internals, and [`status_feed`] answers "what
//!    changed, what is being built, and when will it ship".
//!
//! The crate is deliberately free of network, database, and AI client code:
//! the app supplies the engine and the `GitHub` client, which keeps the whole
//! loop testable in-process.

pub mod disclosure;
pub mod pipeline;
pub mod roadmap;
pub mod submission;
pub mod triage;
pub mod voting;

use std::collections::BTreeMap;

pub use disclosure::DisclosureError;
pub use disclosure::DisclosureGate;
pub use disclosure::DisclosurePolicy;
pub use disclosure::OutboundDraft;
pub use disclosure::OutboundUpdate;
pub use disclosure::PublicLink;
pub use disclosure::RepositoryVisibility;
pub use disclosure::Sensitivity;
pub use pipeline::Actor;
pub use pipeline::Command;
pub use pipeline::FeedbackEvent;
pub use pipeline::FeedbackItem;
pub use pipeline::IssueRef;
pub use pipeline::MaintainerDecision;
pub use pipeline::PullRequestRef;
pub use pipeline::ReleaseLink;
pub use pipeline::Stage;
pub use pipeline::TransitionError;
pub use roadmap::CadenceSnapshot;
pub use roadmap::PublicStatus;
pub use roadmap::RoadmapEntry;
pub use roadmap::ShipWindow;
pub use roadmap::ShippedEntry;
pub use roadmap::StatusFeed;
pub use roadmap::public_item_update;
pub use roadmap::public_status;
pub use roadmap::public_status_line;
pub use roadmap::public_title;
pub use roadmap::ship_window;
pub use roadmap::status_feed;
pub use submission::Attachment;
pub use submission::FeedbackKind;
pub use submission::FeedbackSubmission;
pub use submission::PageContext;
pub use submission::RegisteredApp;
pub use submission::SubmitterIdentity;
use thiserror::Error;
pub use triage::ReproductionOutcome;
pub use triage::RuleBasedTriage;
pub use triage::ScreeningVerdict;
pub use triage::TriageEngine;
pub use triage::TriageFinding;
pub use triage::TriageReport;
pub use triage::screen_untrusted;
pub use voting::VoteTally;
pub use voting::VotingOutcome;
pub use voting::VotingRules;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServiceError {
	#[error("feedback item {0} does not exist")]
	NotFound(String),
	#[error(transparent)]
	Transition(#[from] TransitionError),
	#[error(transparent)]
	Disclosure(#[from] DisclosureError),
}

/// In-process orchestrator for the feedback loop. The hosted app will back
/// this with the database and drive the same commands from HTTP handlers and
/// `GitHub` webhooks; the state machine guarantees stay identical.
pub struct FeedbackService<T: TriageEngine> {
	triage: T,
	policy: DisclosurePolicy,
	rules: VotingRules,
	cadence: CadenceSnapshot,
	items: BTreeMap<String, FeedbackItem>,
}

impl<T: TriageEngine> FeedbackService<T> {
	pub fn new(
		triage: T,
		policy: DisclosurePolicy,
		rules: VotingRules,
		cadence: CadenceSnapshot,
	) -> Self {
		Self {
			triage,
			policy,
			rules,
			cadence,
			items: BTreeMap::new(),
		}
	}

	pub fn policy(&self) -> DisclosurePolicy {
		self.policy
	}

	pub fn item(&self, id: &str) -> Option<&FeedbackItem> {
		self.items.get(id)
	}

	/// Stores a submission and runs the automated prefix of the pipeline:
	/// screening, triage, then either a follow-up question or open voting.
	/// Quarantined items stop until [`Self::resume_triage`].
	pub fn receive(&mut self, submission: FeedbackSubmission) -> Result<Stage, ServiceError> {
		let mut item = FeedbackItem::new(submission, self.rules);
		let stage = if let ScreeningVerdict::InjectionSuspected { .. } =
			screen_untrusted(&item.submission.description)
		{
			item.apply(Command::Quarantine, &Actor::System)?;
			item.stage
		} else {
			item.apply(Command::StartTriage, &Actor::System)?;
			let report = self.triage.triage(&item.submission);
			item.apply(Command::CompleteTriage(report.clone()), &Actor::Ai)?;
			advance_after_triage(&mut item, &report)?;
			item.stage
		};
		self.items.insert(item.submission.id.clone(), item);
		Ok(stage)
	}

	/// A maintainer vouches for a quarantined item and triage proceeds.
	pub fn resume_triage(&mut self, id: &str, maintainer: &str) -> Result<Stage, ServiceError> {
		let report = {
			let item = self.item_mut(id)?;
			item.apply(
				Command::StartTriage,
				&Actor::Maintainer(maintainer.to_owned()),
			)?;
			let submission = item.submission.clone();
			self.triage.triage(&submission)
		};
		let item = self.item_mut(id)?;
		item.apply(Command::CompleteTriage(report.clone()), &Actor::Ai)?;
		advance_after_triage(item, &report)?;
		Ok(item.stage)
	}

	pub fn record_vote(&mut self, id: &str, submitter_id: &str) -> Result<u32, ServiceError> {
		let item = self.item_mut(id)?;
		item.apply(
			Command::RecordVote {
				submitter_id: submitter_id.to_owned(),
			},
			&Actor::User(submitter_id.to_owned()),
		)?;
		Ok(item.votes.total())
	}

	pub fn accept(
		&mut self,
		id: &str,
		decision: MaintainerDecision,
	) -> Result<Stage, ServiceError> {
		let actor = Actor::Maintainer(decision.maintainer.clone());
		let item = self.item_mut(id)?;
		item.apply(Command::Accept(decision), &actor)?;
		Ok(item.stage)
	}

	pub fn decline(
		&mut self,
		id: &str,
		decision: MaintainerDecision,
	) -> Result<Stage, ServiceError> {
		let actor = Actor::Maintainer(decision.maintainer.clone());
		let item = self.item_mut(id)?;
		item.apply(Command::Decline(decision), &actor)?;
		Ok(item.stage)
	}

	pub fn link_issue(
		&mut self,
		id: &str,
		number: u64,
		url: Option<String>,
	) -> Result<Stage, ServiceError> {
		let item = self.item_mut(id)?;
		item.apply(Command::LinkIssue { number, url }, &Actor::System)?;
		Ok(item.stage)
	}

	pub fn start_build(&mut self, id: &str) -> Result<Stage, ServiceError> {
		let item = self.item_mut(id)?;
		item.apply(Command::StartBuild, &Actor::System)?;
		Ok(item.stage)
	}

	pub fn open_pull_request(
		&mut self,
		id: &str,
		number: u64,
		url: String,
	) -> Result<Stage, ServiceError> {
		let item = self.item_mut(id)?;
		item.apply(Command::OpenPullRequest { number, url }, &Actor::System)?;
		Ok(item.stage)
	}

	pub fn mark_shipped(&mut self, id: &str, release: ReleaseLink) -> Result<Stage, ServiceError> {
		let item = self.item_mut(id)?;
		item.apply(Command::MarkShipped(release), &Actor::System)?;
		Ok(item.stage)
	}

	pub fn status_feed(&self) -> Result<StatusFeed, ServiceError> {
		Ok(roadmap::status_feed(
			self.items.values(),
			self.policy,
			&self.cadence,
		)?)
	}

	/// The per-item "what happened to my feedback" view for the widget.
	pub fn public_item_update(&self, id: &str) -> Result<OutboundUpdate, ServiceError> {
		let item = self
			.items
			.get(id)
			.ok_or_else(|| ServiceError::NotFound(id.to_owned()))?;
		Ok(roadmap::public_item_update(item, self.policy)?)
	}

	fn item_mut(&mut self, id: &str) -> Result<&mut FeedbackItem, ServiceError> {
		self.items
			.get_mut(id)
			.ok_or_else(|| ServiceError::NotFound(id.to_owned()))
	}
}

/// Moves an item from triage into discussion (when questions exist) or opens
/// voting. Kept as a free function so both the fresh-intake and the
/// quarantine-resume paths share one definition of "what happens after
/// triage".
fn advance_after_triage(
	item: &mut FeedbackItem,
	report: &TriageReport,
) -> Result<(), TransitionError> {
	if report.questions.is_empty() {
		item.apply(Command::OpenVoting, &Actor::System)
	} else {
		item.apply(Command::AskUser, &Actor::Ai)
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
