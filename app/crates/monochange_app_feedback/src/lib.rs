//! Feedback pipeline for the monochange web app.
//!
//! This crate models the loop between users of third-party apps and the
//! maintainers who build them:
//!
//! 1. **Intake** — a [`RegisteredApp`] submits a bug report or feature
//!    request with a description, the page and element the user pointed at,
//!    and screenshots ([`FeedbackSubmission`]). Similar open items come back
//!    with the receipt so the widget can offer "vote for this instead".
//! 2. **Screening** — untrusted text is checked for prompt-injection markers
//!    before any automated processing; suspect items are quarantined until a
//!    maintainer vouches for them ([`screen_untrusted`]).
//! 3. **Triage** — a [`TriageEngine`] classifies the report, attempts a
//!    reproduction, and asks follow-up questions. The submitter's reply
//!    re-runs triage; once nothing is missing the item opens for votes.
//! 4. **Voting** — votes signal demand but never accept work on their own.
//! 5. **Decision** — only maintainers accept, decline, or fold duplicates;
//!    accepting below the vote threshold requires a recorded override
//!    rationale ([`MaintainerDecision`]).
//! 6. **Handoff** — an accepted item renders a `GitHub` issue, a brief for
//!    the coding agent, and a monochange changeset ([`handoff`]). The
//!    pipeline has no merge or push command: it observes merges and releases
//!    ([`FeedbackService::observe_merge`], [`FeedbackService::observe_release`]).
//! 7. **Status** — every user-facing word passes the [`DisclosureGate`]
//!    under the repository's [`DisclosurePolicy`], so private codebases never
//!    leak internals. Subscribers get a [`Notification`] whenever an item's
//!    public status changes, and [`FeedbackService::status_feed`] answers
//!    "what changed, what is being built, and when will it ship".
//!
//! The crate is free of network, database, and AI client code: the app
//! supplies the engine and the `GitHub` client, which keeps the whole loop
//! testable in-process.

pub mod disclosure;
pub mod discussion;
pub mod handoff;
pub mod pipeline;
pub mod roadmap;
pub mod similarity;
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
pub use disclosure::Redaction;
pub use disclosure::RepositoryVisibility;
pub use disclosure::Sensitivity;
pub use disclosure::Surface;
pub use discussion::Actor;
pub use discussion::DiscussionMessage;
pub use handoff::AgentBrief;
pub use handoff::ChangesetDraft;
pub use handoff::ChangesetTarget;
pub use handoff::HandoffError;
pub use handoff::IssueDraft;
pub use handoff::closing_issue_numbers;
pub use handoff::feedback_trailers;
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
pub use roadmap::FeedRender;
pub use roadmap::PublicStatus;
pub use roadmap::RoadmapEntry;
pub use roadmap::ShipWindow;
pub use roadmap::ShippedEntry;
pub use roadmap::StatusFeed;
pub use roadmap::Withheld;
use serde::Deserialize;
use serde::Serialize;
pub use submission::Attachment;
pub use submission::FeedbackKind;
pub use submission::FeedbackSubmission;
pub use submission::IntakeError;
pub use submission::PageContext;
pub use submission::PinnedElement;
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

/// Items at least this similar to a new description are offered as
/// "vote for this instead" suggestions.
pub const SIMILARITY_THRESHOLD_PERCENT: u8 = 30;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServiceError {
	#[error("feedback item {0} does not exist")]
	NotFound(String),
	#[error("no app is registered with the slug {0}")]
	UnknownApp(String),
	#[error("{0} cannot absorb duplicates")]
	InvalidDuplicateTarget(String),
	#[error(transparent)]
	Intake(#[from] IntakeError),
	#[error(transparent)]
	Transition(#[from] TransitionError),
	#[error(transparent)]
	Disclosure(#[from] DisclosureError),
	#[error(transparent)]
	Handoff(#[from] HandoffError),
}

/// What the widget learns right after submitting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
	pub id: String,
	pub stage: Stage,
	/// Open items that look like the same request, most similar first.
	pub similar: Vec<SimilarItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarItem {
	pub id: String,
	pub title: String,
	pub status: PublicStatus,
	pub votes: u32,
	pub similarity_percent: u8,
}

/// A status change addressed to one subscriber, already through the gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
	pub recipient: String,
	pub item_id: String,
	pub status: PublicStatus,
	pub update: OutboundUpdate,
}

/// Who wrote a public discussion message, without revealing identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicAuthor {
	Submitter,
	Community,
	Maintainer,
	Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMessage {
	pub author: PublicAuthor,
	pub body: String,
	pub attachments: usize,
}

/// A release seen by the release automation, listing the pull requests it
/// contains. monochange release records carry exactly this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseObservation {
	/// The repository that released, when known.
	#[serde(default)]
	pub repository: Option<String>,
	pub version: String,
	pub notes_url: String,
	pub pull_requests: Vec<u64>,
}

/// Everything about a project's feedback that must outlive a request: the
/// voting rules, registered apps, and items. The triage engine, disclosure
/// policy, and release cadence come from the deployment and the current state
/// of the project's repositories, so they are supplied when restoring.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeedbackState {
	pub rules: VotingRules,
	pub apps: Vec<RegisteredApp>,
	pub items: Vec<FeedbackItem>,
}

/// In-process orchestrator for one repository's feedback loop. The hosted
/// app backs this with the database and drives the same methods from HTTP
/// handlers and `GitHub` webhooks; the state machine guarantees stay
/// identical.
pub struct FeedbackService<T: TriageEngine> {
	triage: T,
	policy: DisclosurePolicy,
	rules: VotingRules,
	cadence: CadenceSnapshot,
	apps: BTreeMap<String, RegisteredApp>,
	items: Vec<FeedbackItem>,
	notifications: Vec<Notification>,
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
			apps: BTreeMap::new(),
			items: Vec::new(),
			notifications: Vec::new(),
		}
	}

	/// Restores a service from stored state.
	pub fn from_state(
		triage: T,
		policy: DisclosurePolicy,
		cadence: CadenceSnapshot,
		state: FeedbackState,
	) -> Self {
		Self {
			triage,
			policy,
			rules: state.rules,
			cadence,
			apps: state
				.apps
				.into_iter()
				.map(|app| (app.slug.clone(), app))
				.collect(),
			items: state.items,
			notifications: Vec::new(),
		}
	}

	/// The state to store. Pending notifications are not part of it; drain
	/// and deliver them separately.
	pub fn state(&self) -> FeedbackState {
		FeedbackState {
			rules: self.rules,
			apps: self.apps.values().cloned().collect(),
			items: self.items.clone(),
		}
	}

	pub fn policy(&self) -> DisclosurePolicy {
		self.policy
	}

	/// Applies a repository visibility change, such as the `GitHub`
	/// `repository.privatized` webhook. Every later render uses the new
	/// policy; nothing already rendered is cached.
	pub fn set_policy(&mut self, policy: DisclosurePolicy) {
		self.policy = policy;
	}

	pub fn cadence(&self) -> &CadenceSnapshot {
		&self.cadence
	}

	pub fn set_cadence(&mut self, cadence: CadenceSnapshot) {
		self.cadence = cadence;
	}

	pub fn register_app(&mut self, app: RegisteredApp) {
		self.apps.insert(app.slug.clone(), app);
	}

	pub fn app(&self, slug: &str) -> Option<&RegisteredApp> {
		self.apps.get(slug)
	}

	pub fn apps(&self) -> impl Iterator<Item = &RegisteredApp> {
		self.apps.values()
	}

	/// Stops an app from submitting. Its earlier feedback stays.
	pub fn remove_app(&mut self, slug: &str) -> bool {
		self.apps.remove(slug).is_some()
	}

	pub fn items(&self) -> &[FeedbackItem] {
		&self.items
	}

	pub fn item(&self, id: &str) -> Option<&FeedbackItem> {
		self.items.iter().find(|item| item.id == id)
	}

	/// Listed items whose words overlap `text`, most similar first. Only
	/// gated titles are returned, so suggestions are safe to show anyone.
	pub fn similar(&self, text: &str, limit: usize) -> Vec<SimilarItem> {
		let wanted = similarity::keywords(text);
		let mut matches: Vec<SimilarItem> = self
			.items
			.iter()
			.filter(|item| {
				roadmap::is_listed(item) && !matches!(item.stage, Stage::Declined | Stage::Closed)
			})
			.filter_map(|item| {
				let percent = similarity::similarity_percent(
					&wanted,
					&similarity::keywords(&item.submission.description),
				);
				if percent < SIMILARITY_THRESHOLD_PERCENT {
					return None;
				}
				let entry = roadmap::roadmap_entry(item, self.policy, &self.cadence).ok()?;
				Some(SimilarItem {
					id: entry.id,
					title: entry.title,
					status: entry.status,
					votes: entry.votes,
					similarity_percent: percent,
				})
			})
			.collect();
		matches.sort_by(|left, right| {
			right
				.similarity_percent
				.cmp(&left.similarity_percent)
				.then(right.votes.cmp(&left.votes))
		});
		matches.truncate(limit);
		matches
	}

	/// Stores a submission and runs the automated prefix of the pipeline:
	/// screening, triage, then either a follow-up question or open voting.
	/// Quarantined items stop until [`Self::resume_triage`].
	pub fn receive(&mut self, submission: FeedbackSubmission) -> Result<Receipt, ServiceError> {
		submission.validate()?;
		if !self.apps.contains_key(&submission.app_slug) {
			return Err(ServiceError::UnknownApp(submission.app_slug));
		}
		let similar = self.similar(&submission.description, 3);
		// Items are never deleted, so the count names the next one.
		let id = format!("fb-{}", self.items.len() + 1);
		let mut item = FeedbackItem::new(id.clone(), submission, self.rules);
		if let ScreeningVerdict::InjectionSuspected { .. } =
			screen_untrusted(&item.submission.description)
		{
			item.apply(Command::Quarantine, &Actor::System)?;
		} else {
			item.apply(Command::StartTriage, &Actor::System)?;
			run_triage(&self.triage, &mut item)?;
		}
		let stage = item.stage;
		self.items.push(item);
		Ok(Receipt { id, stage, similar })
	}

	/// A maintainer vouches for a quarantined item and triage proceeds.
	pub fn resume_triage(&mut self, id: &str, maintainer: &str) -> Result<Stage, ServiceError> {
		self.mutate(id, |item, triage| {
			item.apply(
				Command::StartTriage,
				&Actor::Maintainer(maintainer.to_owned()),
			)?;
			run_triage(triage, item)
		})
	}

	/// Adds a message to the item's thread. When the submitter answers
	/// follow-up questions, triage re-runs with the answer and the item opens
	/// for votes once nothing is missing.
	pub fn reply(
		&mut self,
		id: &str,
		author: &Actor,
		body: &str,
		attachments: Vec<Attachment>,
	) -> Result<Stage, ServiceError> {
		self.mutate(id, |item, triage| {
			item.apply(
				Command::PostMessage {
					body: body.to_owned(),
					attachments,
				},
				author,
			)?;
			let submitter_answered = *author
				== Actor::User(item.submission.submitter.anonymous_id.clone())
				&& item.discussion.last().is_some_and(|message| !message.held);
			if item.stage == Stage::Discussing && submitter_answered {
				let report = triage.triage(&item.submission, &item.visible_discussion());
				let ready = report.questions.is_empty();
				item.apply(Command::CompleteTriage(report), &Actor::Ai)?;
				if ready {
					item.apply(Command::OpenVoting, &Actor::System)?;
				}
			}
			Ok(item.stage)
		})
	}

	/// A maintainer opens voting without waiting for the submitter.
	pub fn open_voting(&mut self, id: &str, maintainer: &str) -> Result<Stage, ServiceError> {
		self.apply(
			id,
			Command::OpenVoting,
			&Actor::Maintainer(maintainer.to_owned()),
		)
	}

	pub fn vote(&mut self, id: &str, voter: &str) -> Result<u32, ServiceError> {
		self.mutate(id, |item, _| {
			item.apply(Command::RecordVote, &Actor::User(voter.to_owned()))?;
			Ok(item.votes.total())
		})
	}

	pub fn retract_vote(&mut self, id: &str, voter: &str) -> Result<u32, ServiceError> {
		self.mutate(id, |item, _| {
			item.apply(Command::RetractVote, &Actor::User(voter.to_owned()))?;
			Ok(item.votes.total())
		})
	}

	pub fn edit_summary(
		&mut self,
		id: &str,
		maintainer: &str,
		summary: &str,
	) -> Result<Stage, ServiceError> {
		self.apply(
			id,
			Command::EditSummary(summary.to_owned()),
			&Actor::Maintainer(maintainer.to_owned()),
		)
	}

	pub fn accept(
		&mut self,
		id: &str,
		decision: MaintainerDecision,
	) -> Result<Stage, ServiceError> {
		let actor = Actor::Maintainer(decision.maintainer.clone());
		self.apply(id, Command::Accept(decision), &actor)
	}

	pub fn decline(
		&mut self,
		id: &str,
		decision: MaintainerDecision,
	) -> Result<Stage, ServiceError> {
		let actor = Actor::Maintainer(decision.maintainer.clone());
		self.apply(id, Command::Decline(decision), &actor)
	}

	pub fn close(&mut self, id: &str, maintainer: &str) -> Result<Stage, ServiceError> {
		self.apply(
			id,
			Command::Close,
			&Actor::Maintainer(maintainer.to_owned()),
		)
	}

	/// Folds `id` into `canonical`: the duplicate closes, and its submitter
	/// and voters become votes and subscribers on the canonical item.
	pub fn mark_duplicate(
		&mut self,
		id: &str,
		canonical: &str,
		maintainer: &str,
	) -> Result<Stage, ServiceError> {
		let target = self
			.item(canonical)
			.ok_or_else(|| ServiceError::NotFound(canonical.to_owned()))?;
		if !target.stage.is_open() || target.duplicate_of.is_some() {
			return Err(ServiceError::InvalidDuplicateTarget(canonical.to_owned()));
		}
		let actor = Actor::Maintainer(maintainer.to_owned());
		let duplicate = self.mutate(id, |item, _| {
			item.apply(
				Command::MarkDuplicate {
					of: canonical.to_owned(),
				},
				&actor,
			)?;
			Ok(item.clone())
		})?;
		self.mutate(canonical, |item, _| {
			item.absorb_duplicate(&duplicate, &actor);
			Ok(())
		})?;
		Ok(duplicate.stage)
	}

	pub fn link_issue(&mut self, id: &str, issue: IssueRef) -> Result<Stage, ServiceError> {
		self.apply(id, Command::LinkIssue(issue), &Actor::System)
	}

	pub fn start_build(&mut self, id: &str) -> Result<Stage, ServiceError> {
		self.apply(id, Command::StartBuild, &Actor::System)
	}

	pub fn open_pull_request(
		&mut self,
		id: &str,
		pull_request: PullRequestRef,
	) -> Result<Stage, ServiceError> {
		self.apply(id, Command::OpenPullRequest(pull_request), &Actor::System)
	}

	/// Handles an opened pull request (the `pull_request` webhook). Each
	/// accepted item it delivers moves to review: items whose issue in the
	/// same repository it closes (`Fixes #12`), and items it names with a
	/// `Feedback-Item:` trailer whose issue lives in that repository. Item ids
	/// are per project and a repository can belong to several projects, so
	/// the issue's repository is what makes a trailer unambiguous. Returns the
	/// advanced item ids.
	pub fn observe_pull_request(
		&mut self,
		pull_request: &PullRequestRef,
		body: &str,
	) -> Result<Vec<String>, ServiceError> {
		let repository = pull_request.repository.as_deref();
		let closes = repository
			.map(|repository| handoff::closing_issue_numbers(body, repository))
			.unwrap_or_default();
		let named = handoff::feedback_trailers(body);
		let delivered: Vec<(String, Stage)> = self
			.items
			.iter()
			.filter(|item| {
				matches!(item.stage, Stage::Accepted | Stage::Building)
					&& item.issue.as_ref().is_some_and(|issue| {
						issue.is_in(repository)
							&& (closes.contains(&issue.number) || named.contains(&item.id))
					})
			})
			.map(|item| (item.id.clone(), item.stage))
			.collect();
		for (id, stage) in &delivered {
			if *stage == Stage::Accepted {
				self.start_build(id)?;
			}
			self.open_pull_request(id, pull_request.clone())?;
		}
		Ok(delivered.into_iter().map(|(id, _)| id).collect())
	}

	/// Handles a merged pull request (the `pull_request.closed` webhook with
	/// `merged = true`). Returns the item it belonged to, if any.
	pub fn observe_merge(
		&mut self,
		repository: Option<&str>,
		pull_request: u64,
	) -> Result<Option<String>, ServiceError> {
		let Some(id) = self.item_in_review(repository, pull_request, &[Stage::InReview]) else {
			return Ok(None);
		};
		self.apply(&id, Command::MarkMerged, &Actor::System)?;
		Ok(Some(id))
	}

	/// Handles a published release: every item whose pull request the release
	/// contains is marked shipped. Returns the shipped item ids.
	pub fn observe_release(
		&mut self,
		release: &ReleaseObservation,
	) -> Result<Vec<String>, ServiceError> {
		let mut shipped = Vec::new();
		for &number in &release.pull_requests {
			let Some(id) = self.item_in_review(
				release.repository.as_deref(),
				number,
				&[Stage::InReview, Stage::Merged],
			) else {
				continue;
			};
			let link = ReleaseLink {
				version: release.version.clone(),
				notes_url: release.notes_url.clone(),
			};
			self.apply(&id, Command::MarkShipped(link), &Actor::System)?;
			shipped.push(id);
		}
		Ok(shipped)
	}

	/// Handles a release published without a list of pull requests (the
	/// `release.published` webhook): every merged item whose pull request
	/// belongs to `repository` shipped in it. Returns the shipped item ids.
	pub fn ship_merged(
		&mut self,
		repository: Option<&str>,
		release: &ReleaseLink,
	) -> Result<Vec<String>, ServiceError> {
		let merged: Vec<String> = self
			.items
			.iter()
			.filter(|item| {
				item.stage == Stage::Merged
					&& item.pull_request.as_ref().is_some_and(|pull_request| {
						pull_request.matches(repository, pull_request.number)
					})
			})
			.map(|item| item.id.clone())
			.collect();
		for id in &merged {
			self.apply(id, Command::MarkShipped(release.clone()), &Actor::System)?;
		}
		Ok(merged)
	}

	pub fn issue_draft(&self, id: &str) -> Result<IssueDraft, ServiceError> {
		Ok(handoff::issue_draft(
			self.find(id)?,
			self.policy.visibility,
		)?)
	}

	pub fn agent_brief(
		&self,
		id: &str,
		target: &ChangesetTarget,
	) -> Result<AgentBrief, ServiceError> {
		Ok(handoff::agent_brief(
			self.find(id)?,
			self.policy.visibility,
			target,
		)?)
	}

	/// The public roadmap and shipped list, plus what was withheld.
	pub fn status_feed(&self) -> FeedRender {
		roadmap::status_feed(&self.items, self.policy, &self.cadence)
	}

	/// The per-item "what happened to my feedback" view for the widget.
	pub fn public_item_update(&self, id: &str) -> Result<OutboundUpdate, ServiceError> {
		Ok(roadmap::public_item_update(self.find(id)?, self.policy)?)
	}

	/// The discussion as other users see it: held messages are left out and
	/// every body is redacted for the portal.
	pub fn public_thread(&self, id: &str) -> Result<Vec<PublicMessage>, ServiceError> {
		let item = self.find(id)?;
		let submitter = Actor::User(item.submission.submitter.anonymous_id.clone());
		Ok(item
			.visible_discussion()
			.into_iter()
			.map(|message| {
				let author = match &message.author {
					author if *author == submitter => PublicAuthor::Submitter,
					Actor::User(_) => PublicAuthor::Community,
					Actor::Maintainer(_) => PublicAuthor::Maintainer,
					Actor::Ai | Actor::System => PublicAuthor::Assistant,
				};
				PublicMessage {
					author,
					body: disclosure::redact(
						&message.body,
						Surface::Portal,
						self.policy.visibility,
					)
					.text,
					attachments: message.attachments.len(),
				}
			})
			.collect())
	}

	/// Notifications queued since the last drain, oldest first.
	pub fn drain_notifications(&mut self) -> Vec<Notification> {
		std::mem::take(&mut self.notifications)
	}

	fn find(&self, id: &str) -> Result<&FeedbackItem, ServiceError> {
		self.item(id)
			.ok_or_else(|| ServiceError::NotFound(id.to_owned()))
	}

	fn item_in_review(
		&self,
		repository: Option<&str>,
		pull_request: u64,
		stages: &[Stage],
	) -> Option<String> {
		self.items
			.iter()
			.find(|item| {
				stages.contains(&item.stage)
					&& item
						.pull_request
						.as_ref()
						.is_some_and(|reference| reference.matches(repository, pull_request))
			})
			.map(|item| item.id.clone())
	}

	fn apply(&mut self, id: &str, command: Command, actor: &Actor) -> Result<Stage, ServiceError> {
		self.mutate(id, |item, _| {
			item.apply(command, actor)?;
			Ok(item.stage)
		})
	}

	/// Runs `change` against one item and notifies its subscribers when the
	/// item's public status line changed.
	///
	/// A notification the gate refuses is not sent: the same item is then
	/// withheld from the feed, which surfaces it to maintainers, and nothing
	/// unscreened reaches users.
	fn mutate<R>(
		&mut self,
		id: &str,
		change: impl FnOnce(&mut FeedbackItem, &T) -> Result<R, ServiceError>,
	) -> Result<R, ServiceError> {
		let index = self
			.items
			.iter()
			.position(|item| item.id == id)
			.ok_or_else(|| ServiceError::NotFound(id.to_owned()))?;
		let item = &mut self.items[index];
		let before = roadmap::public_status_line(item.stage);
		let result = change(item, &self.triage)?;
		if roadmap::public_status_line(item.stage) != before
			&& let Ok(update) = roadmap::public_item_update(item, self.policy)
		{
			let status = roadmap::public_status(item.stage);
			self.notifications
				.extend(item.subscribers.iter().map(|recipient| {
					Notification {
						recipient: recipient.clone(),
						item_id: item.id.clone(),
						status,
						update: update.clone(),
					}
				}));
		}
		Ok(result)
	}
}

/// Triages a freshly triaging item, then moves it into discussion (when
/// questions remain) or opens voting. Shared by intake and the quarantine
/// resume so both agree on "what happens after triage".
fn run_triage<T: TriageEngine>(triage: &T, item: &mut FeedbackItem) -> Result<Stage, ServiceError> {
	let report = triage.triage(&item.submission, &item.visible_discussion());
	let has_questions = !report.questions.is_empty();
	item.apply(Command::CompleteTriage(report), &Actor::Ai)?;
	if has_questions {
		item.apply(Command::AskUser, &Actor::Ai)?;
	} else {
		item.apply(Command::OpenVoting, &Actor::System)?;
	}
	Ok(item.stage)
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
