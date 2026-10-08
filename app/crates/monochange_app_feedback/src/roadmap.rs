//! User-facing roadmap and status rendering. Every entry passes through the
//! disclosure gate, so the answer to "what changed, what is being built, and
//! when will it ship" never leaks private-repo internals.

use serde::Deserialize;
use serde::Serialize;

use crate::disclosure::DisclosureError;
use crate::disclosure::DisclosureGate;
use crate::disclosure::DisclosurePolicy;
use crate::disclosure::OutboundDraft;
use crate::disclosure::OutboundUpdate;
use crate::disclosure::PublicLink;
use crate::pipeline::FeedbackItem;
use crate::pipeline::IssueRef;
use crate::pipeline::Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicStatus {
	UnderReview,
	Planned,
	InProgress,
	Shipped,
	Declined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipWindow {
	Scheduled { label: String },
	Unscheduled,
}

/// Snapshot of the repository's release cadence as known by the release
/// automation. `next_window_label` might be "next scheduled release" or a
/// version hint like "v2.4 — end of October".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CadenceSnapshot {
	pub next_window_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadmapEntry {
	pub title: String,
	pub status: PublicStatus,
	pub votes: u32,
	pub ship_window: ShipWindow,
	pub links: Vec<PublicLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShippedEntry {
	pub title: String,
	pub version: String,
	pub notes_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusFeed {
	pub roadmap: Vec<RoadmapEntry>,
	pub shipped: Vec<ShippedEntry>,
}

pub fn public_status(stage: &Stage) -> PublicStatus {
	match stage {
		Stage::Received
		| Stage::Quarantined
		| Stage::Triaging
		| Stage::Discussing
		| Stage::Voting => PublicStatus::UnderReview,
		Stage::Accepted => PublicStatus::Planned,
		Stage::Building | Stage::InReview => PublicStatus::InProgress,
		Stage::Shipped => PublicStatus::Shipped,
		Stage::Declined | Stage::Closed => PublicStatus::Declined,
	}
}

pub fn public_status_line(stage: &Stage) -> &'static str {
	match stage {
		// Quarantine is a moderation state, not user-visible; publicly the
		// item is simply still waiting for review.
		Stage::Received | Stage::Quarantined => "Received and waiting for review",
		Stage::Triaging => "Being investigated",
		Stage::Discussing => "We have follow-up questions",
		Stage::Voting => "Open for votes",
		Stage::Accepted => "Accepted and scheduled",
		Stage::Declined => "Declined",
		Stage::Building => "In development",
		Stage::InReview => "In review",
		Stage::Shipped => "Shipped",
		Stage::Closed => "Closed",
	}
}

pub fn public_title(item: &FeedbackItem) -> String {
	let summary = item.triage.as_ref().map_or_else(
		|| first_line(&item.submission.description),
		|report| report.product_summary.clone(),
	);
	truncate_title(&summary)
}

/// Titles cap at word boundaries so the disclosure gate always sees whole
/// tokens — a char-boundary cut could split an internal path in half after
/// redaction ran. A single oversized word falls back to character truncation.
fn truncate_title(summary: &str) -> String {
	const TITLE_LIMIT: usize = 96;
	if summary.chars().count() <= TITLE_LIMIT {
		return summary.to_owned();
	}
	let mut result = String::new();
	let mut length = 0usize;
	for word in summary.split(' ') {
		let word_length = word.chars().count();
		let separator = usize::from(!result.is_empty());
		if length + separator + word_length > TITLE_LIMIT {
			break;
		}
		if separator == 1 {
			result.push(' ');
		}
		result.push_str(word);
		length += separator + word_length;
	}
	if result.is_empty() {
		return summary.chars().take(TITLE_LIMIT).collect();
	}
	result
}

fn first_line(text: &str) -> String {
	text.lines().next().unwrap_or_default().to_owned()
}

fn links_for(item: &FeedbackItem) -> Vec<PublicLink> {
	let mut links = Vec::new();
	if let Some(IssueRef { url: Some(url), .. }) = &item.issue {
		links.push(PublicLink::Issue(url.clone()));
	}
	if let Some(pull_request) = &item.pull_request {
		links.push(PublicLink::PullRequest(pull_request.url.clone()));
	}
	if let Some(release) = &item.release {
		links.push(PublicLink::ReleaseNotes(release.notes_url.clone()));
	}
	links
}

pub fn ship_window(item: &FeedbackItem, cadence: &CadenceSnapshot) -> ShipWindow {
	match item.stage {
		Stage::Shipped => {
			match &item.release {
				Some(release) => {
					ShipWindow::Scheduled {
						label: release.version.clone(),
					}
				}
				None => ShipWindow::Unscheduled,
			}
		}
		Stage::Accepted | Stage::Building | Stage::InReview => {
			match &cadence.next_window_label {
				Some(label) => {
					ShipWindow::Scheduled {
						label: label.clone(),
					}
				}
				None => ShipWindow::Unscheduled,
			}
		}
		_ => ShipWindow::Unscheduled,
	}
}

pub fn roadmap_entry(
	item: &FeedbackItem,
	policy: DisclosurePolicy,
	cadence: &CadenceSnapshot,
) -> Result<RoadmapEntry, DisclosureError> {
	let update = DisclosureGate::publish(outbound_draft(item), policy)?;
	Ok(RoadmapEntry {
		title: update.title,
		status: public_status(&item.stage),
		votes: item.votes.total(),
		ship_window: ship_window(item, cadence),
		links: update.links,
	})
}

/// The "what happened to my feedback" view rendered inside the widget.
pub fn public_item_update(
	item: &FeedbackItem,
	policy: DisclosurePolicy,
) -> Result<OutboundUpdate, DisclosureError> {
	DisclosureGate::publish(outbound_draft(item), policy)
}

fn outbound_draft(item: &FeedbackItem) -> OutboundDraft {
	OutboundDraft {
		title: public_title(item),
		body: public_status_line(&item.stage).to_owned(),
		links: links_for(item),
		technical_detail: None,
	}
}

pub fn status_feed<'a>(
	items: impl Iterator<Item = &'a FeedbackItem>,
	policy: DisclosurePolicy,
	cadence: &CadenceSnapshot,
) -> Result<StatusFeed, DisclosureError> {
	let mut feed = StatusFeed::default();
	for item in items {
		if item.stage == Stage::Shipped {
			let update = DisclosureGate::publish(outbound_draft(item), policy)?;
			let release = item.release.as_ref();
			feed.shipped.push(ShippedEntry {
				title: update.title,
				version: release
					.map_or_else(|| "unreleased".to_owned(), |link| link.version.clone()),
				notes_url: release.map_or_else(String::new, |link| link.notes_url.clone()),
			});
		} else {
			feed.roadmap.push(roadmap_entry(item, policy, cadence)?);
		}
	}
	Ok(feed)
}

#[cfg(test)]
#[path = "__tests__/roadmap_tests.rs"]
mod tests;
