//! User-facing roadmap and status rendering — the answer to "what changed,
//! what is being built, and when will it ship". Every entry passes through
//! the [`DisclosureGate`], so it never leaks private-repository internals.

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
use crate::submission::FeedbackKind;

/// Longest public title, in characters.
pub const TITLE_LIMIT: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicStatus {
	InProgress,
	Planned,
	UnderReview,
	Shipped,
	Declined,
}

/// The honest answer to "when will it ship". Dates are only promised once
/// the code has merged; before that an accepted item is planned, not
/// scheduled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "window", rename_all = "snake_case")]
pub enum ShipWindow {
	Released {
		version: String,
	},
	/// Merged and waiting for the next release, labelled from the release
	/// cadence when one is known.
	NextRelease {
		label: Option<String>,
	},
	Planned,
	Unscheduled,
}

/// The repository's release cadence as known by the release automation,
/// such as `"v2.4 · around 21 October"`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CadenceSnapshot {
	pub next_release_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadmapEntry {
	pub id: String,
	pub kind: FeedbackKind,
	pub title: String,
	pub status: PublicStatus,
	pub status_line: String,
	pub votes: u32,
	pub ship_window: ShipWindow,
	pub links: Vec<PublicLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShippedEntry {
	pub id: String,
	pub title: String,
	pub version: String,
	pub notes_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusFeed {
	pub roadmap: Vec<RoadmapEntry>,
	pub shipped: Vec<ShippedEntry>,
}

/// An item the gate refused to publish. Maintainer-facing only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Withheld {
	pub id: String,
	pub reason: String,
}

/// The public feed plus the maintainer-side record of what was left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedRender {
	pub feed: StatusFeed,
	pub withheld: Vec<Withheld>,
}

pub fn public_status(stage: Stage) -> PublicStatus {
	match stage {
		Stage::Received
		| Stage::Quarantined
		| Stage::Triaging
		| Stage::Discussing
		| Stage::Voting => PublicStatus::UnderReview,
		Stage::Accepted => PublicStatus::Planned,
		Stage::Building | Stage::InReview | Stage::Merged => PublicStatus::InProgress,
		Stage::Shipped => PublicStatus::Shipped,
		Stage::Declined | Stage::Closed => PublicStatus::Declined,
	}
}

pub fn public_status_line(stage: Stage) -> &'static str {
	match stage {
		// Quarantine is a moderation state; publicly the item is simply
		// still waiting for review.
		Stage::Received | Stage::Quarantined => "Received and waiting for review",
		Stage::Triaging => "Being investigated",
		Stage::Discussing => "We have a follow-up question",
		Stage::Voting => "Open for votes",
		Stage::Accepted => "Accepted and planned",
		Stage::Declined => "Declined",
		Stage::Building => "In development",
		Stage::InReview => "In review",
		Stage::Merged => "Done, shipping in the next release",
		Stage::Shipped => "Shipped",
		Stage::Closed => "Closed",
	}
}

/// The public title, or `None` while the item has no reviewed summary.
pub fn public_title(item: &FeedbackItem) -> Option<String> {
	let summary = item.summary_override.as_deref().or_else(|| {
		item.triage
			.as_ref()
			.map(|report| report.product_summary.as_str())
	})?;
	Some(truncate_title(summary))
}

/// Whether the item belongs on the public roadmap: it has been triaged (so
/// its title is a reviewed summary rather than raw user text) and it was not
/// folded into another item.
pub fn is_listed(item: &FeedbackItem) -> bool {
	!matches!(
		item.stage,
		Stage::Received | Stage::Quarantined | Stage::Triaging
	) && item.duplicate_of.is_none()
		&& public_title(item).is_some()
}

/// Titles cap at word boundaries so redaction always sees whole tokens — a
/// character cut could split an internal path in half and hide it from the
/// classifier. A single oversized word falls back to a character cut.
fn truncate_title(summary: &str) -> String {
	if summary.chars().count() <= TITLE_LIMIT {
		return summary.to_owned();
	}
	let budget = TITLE_LIMIT - 1;
	let mut result = String::new();
	for word in summary.split(' ') {
		let separator = usize::from(!result.is_empty());
		if result.chars().count() + separator + word.chars().count() > budget {
			break;
		}
		if separator == 1 {
			result.push(' ');
		}
		result.push_str(word);
	}
	if result.is_empty() {
		result = summary.chars().take(budget).collect();
	}
	result.push('…');
	result
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
	match (item.stage, &item.release) {
		(Stage::Shipped, Some(release)) => {
			ShipWindow::Released {
				version: release.version.clone(),
			}
		}
		(Stage::Merged, _) => {
			ShipWindow::NextRelease {
				label: cadence.next_release_label.clone(),
			}
		}
		(Stage::Accepted | Stage::Building | Stage::InReview, _) => ShipWindow::Planned,
		_ => ShipWindow::Unscheduled,
	}
}

fn neutral_title(item: &FeedbackItem) -> &'static str {
	match item.submission.kind {
		FeedbackKind::BugReport => "Your problem report",
		FeedbackKind::FeatureRequest => "Your feature request",
	}
}

fn outbound_draft(item: &FeedbackItem, title: String) -> OutboundDraft {
	let status = public_status_line(item.stage);
	let body = match (&item.duplicate_of, item.stage, &item.decision) {
		(Some(_), ..) => {
			"Merged into a matching request. Your vote moved with it, so you will hear about it there."
				.to_owned()
		}
		(None, Stage::Declined, Some(decision)) => format!("{status}: {}", decision.rationale),
		_ => status.to_owned(),
	};
	OutboundDraft {
		title,
		body,
		links: links_for(item),
		technical_detail: None,
	}
}

pub fn roadmap_entry(
	item: &FeedbackItem,
	policy: DisclosurePolicy,
	cadence: &CadenceSnapshot,
) -> Result<RoadmapEntry, DisclosureError> {
	let title = public_title(item).unwrap_or_else(|| neutral_title(item).to_owned());
	let update = DisclosureGate::publish(outbound_draft(item, title), policy)?;
	Ok(RoadmapEntry {
		id: item.id.clone(),
		kind: item
			.triage
			.as_ref()
			.map_or(item.submission.kind, |report| report.classification),
		title: update.title,
		status: public_status(item.stage),
		status_line: update.body,
		votes: item.votes.total(),
		ship_window: ship_window(item, cadence),
		links: update.links,
	})
}

/// The "what happened to my feedback" view shown to the submitter and
/// subscribers. Items without a reviewed summary get a neutral title, so a
/// quarantined description is never echoed back through the portal.
pub fn public_item_update(
	item: &FeedbackItem,
	policy: DisclosurePolicy,
) -> Result<OutboundUpdate, DisclosureError> {
	let title = public_title(item).unwrap_or_else(|| neutral_title(item).to_owned());
	DisclosureGate::publish(outbound_draft(item, title), policy)
}

/// Builds the public feed. One item that fails the gate is withheld and
/// reported to maintainers rather than taking the whole feed down.
pub fn status_feed<'a>(
	items: impl IntoIterator<Item = &'a FeedbackItem>,
	policy: DisclosurePolicy,
	cadence: &CadenceSnapshot,
) -> FeedRender {
	let mut render = FeedRender::default();
	for item in items.into_iter().filter(|item| is_listed(item)) {
		let entry = match roadmap_entry(item, policy, cadence) {
			Ok(entry) => entry,
			Err(error) => {
				render.withheld.push(Withheld {
					id: item.id.clone(),
					reason: error.to_string(),
				});
				continue;
			}
		};
		match (&item.release, item.stage) {
			(Some(release), Stage::Shipped) => {
				render.feed.shipped.push(ShippedEntry {
					id: entry.id,
					title: entry.title,
					version: release.version.clone(),
					notes_url: release.notes_url.clone(),
				});
			}
			_ => render.feed.roadmap.push(entry),
		}
	}
	render.feed.roadmap.sort_by(|left, right| {
		left.status
			.cmp(&right.status)
			.then(right.votes.cmp(&left.votes))
	});
	render.feed.shipped.reverse();
	render
}

#[cfg(test)]
#[path = "__tests__/roadmap_tests.rs"]
mod tests;
