//! The portal shows every roadmap state, thread, and draft helper users meet.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;

use leptos::reactive::computed::ScopedFuture;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::Notification;
use monochange_app_feedback::OutboundUpdate;
use monochange_app_feedback::PublicMessage;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::ShippedEntry;
use monochange_app_feedback::Stage;
use monochange_app_feedback::StatusFeed;

use super::*;
use crate::server_fns::portal::OwnItem;
use crate::tests::feedback::context;
use crate::tests::feedback::state;
use crate::tests::feedback::submit;
use crate::tests::render;

fn entry(id: &str, status: PublicStatus, ship_window: ShipWindow) -> RoadmapEntry {
	RoadmapEntry {
		id: id.to_owned(),
		kind: FeedbackKind::FeatureRequest,
		title: format!("Request {id}"),
		status,
		status_line: "Status".to_owned(),
		votes: 5,
		ship_window,
		links: Vec::new(),
	}
}

fn update(title: &str) -> OutboundUpdate {
	OutboundUpdate {
		title: title.to_owned(),
		body: "Accepted and planned".to_owned(),
		links: Vec::new(),
		technical_detail: None,
	}
}

fn snapshot(feed: StatusFeed) -> PortalSnapshot {
	PortalSnapshot {
		organization: "alice".to_owned(),
		project_slug: "pocketbook".to_owned(),
		project_name: "Pocketbook".to_owned(),
		description: String::new(),
		visibility: RepositoryVisibility::Private,
		feed,
		voted: Vec::new(),
		threads: BTreeMap::new(),
		own: Vec::new(),
		updates: Vec::new(),
	}
}

async fn render_portal(snapshot: PortalSnapshot, shared: Option<&'static str>) -> String {
	render(move || {
		view! {
			<PortalContent
				snapshot=snapshot.clone()
				share=ServerAction::new()
				vote=ServerAction::new()
				reply=ServerAction::new()
				shared=shared.map(str::to_owned)
			/>
		}
	})
	.await
}

#[tokio::test]
async fn the_roadmap_shows_ship_windows_links_votes_and_every_voice() {
	let mut shipped = entry(
		"fb-1",
		PublicStatus::InProgress,
		ShipWindow::Released {
			version: "1.2.0".to_owned(),
		},
	);
	shipped.links = vec![
		PublicLink::Issue("https://github.com/alice/web/issues/1".to_owned()),
		PublicLink::PullRequest("https://github.com/alice/web/pull/2".to_owned()),
		PublicLink::ReleaseNotes("https://example.com/notes".to_owned()),
	];
	let feed = StatusFeed {
		roadmap: vec![
			shipped,
			entry(
				"fb-2",
				PublicStatus::Planned,
				ShipWindow::NextRelease {
					label: Some("v2.4 · around 21 October".to_owned()),
				},
			),
			entry(
				"fb-3",
				PublicStatus::Planned,
				ShipWindow::NextRelease { label: None },
			),
			entry("fb-4", PublicStatus::UnderReview, ShipWindow::Planned),
			entry("fb-5", PublicStatus::Declined, ShipWindow::Unscheduled),
		],
		shipped: vec![ShippedEntry {
			id: "fb-0".to_owned(),
			title: "Export to CSV".to_owned(),
			version: "1.1.0".to_owned(),
			notes_url: "https://example.com/1.1.0".to_owned(),
		}],
	};
	let mut snapshot = snapshot(feed);
	snapshot.voted = vec!["fb-4".to_owned()];
	snapshot.threads.insert(
		"fb-1".to_owned(),
		[
			(PublicAuthor::Submitter, "First"),
			(PublicAuthor::Community, "Second"),
			(PublicAuthor::Maintainer, "Third"),
			(PublicAuthor::Assistant, "Fourth"),
		]
		.into_iter()
		.map(|(author, body)| {
			PublicMessage {
				author,
				body: body.to_owned(),
				attachments: 0,
			}
		})
		.collect(),
	);
	snapshot.own = vec![
		OwnItem {
			id: "fb-6".to_owned(),
			stage: Stage::Discussing,
			update: update("Dark mode"),
			question: Some("What outcome would you like?".to_owned()),
		},
		OwnItem {
			id: "fb-7".to_owned(),
			stage: Stage::Voting,
			update: update("Bulk export"),
			question: None,
		},
	];
	snapshot.updates = vec![Notification {
		recipient: "v-1".to_owned(),
		item_id: "fb-7".to_owned(),
		status: PublicStatus::Planned,
		update: update("Bulk [redacted] export"),
	}];
	let html = render_portal(snapshot, Some("fb-7")).await;

	for expected in [
		"It's filed as fb-7",
		"Shipped in v1.2.0",
		"Ships v2.4 · around 21 October",
		"Ships in the next release",
		"Planned · no date yet",
		">Issue<",
		">Pull request<",
		">Release notes<",
		"Reporter",
		"Someone else",
		">Team<",
		">Assistant<",
		"Remove your vote (5)",
		"Recently shipped",
		"We have a question",
		"Updates",
		"class=\"redaction\"",
	] {
		assert!(html.contains(expected), "missing {expected}: {html}");
	}
	assert!(!html.contains("Nothing on the roadmap yet"));
}

#[tokio::test]
async fn an_empty_portal_invites_the_first_idea() {
	let html = render_portal(snapshot(StatusFeed::default()), None).await;
	assert!(html.contains("Nothing on the roadmap yet. Be the first to share an idea."));
	assert!(html.contains("What you share appears here"));
	assert!(!html.contains("Recently shipped"));
}

#[tokio::test]
async fn similar_requests_are_offered_for_long_drafts_only() {
	let state = state().await;
	submit(
		&state,
		"v-1",
		FeedbackKind::FeatureRequest,
		"Dark mode for invoices",
	)
	.await;
	let (owner, _) = context(&state, None, Some("token-a"));
	let short = owner
		.with(|| {
			ScopedFuture::new(similar_to_draft(
				"alice".to_owned(),
				"pocketbook".to_owned(),
				"dark".to_owned(),
			))
		})
		.await
		.unwrap();
	assert!(short.is_empty());
	let matches = owner
		.with(|| {
			ScopedFuture::new(similar_to_draft(
				"alice".to_owned(),
				"pocketbook".to_owned(),
				"dark mode invoices".to_owned(),
			))
		})
		.await
		.unwrap();
	assert_eq!(matches[0].id, "fb-1");

	assert!(similar_notice(Vec::new()).is_none());
	let html = render(move || similar_notice(matches.clone())).await;
	assert!(html.contains("Looks familiar"), "{html}");
	assert!(html.contains("href=\"#item-fb-1\""), "{html}");
}

#[tokio::test]
async fn previews_render_drafts_and_explain_empty_or_failed_ones() {
	assert_eq!(preview_draft(None).await.unwrap(), "");
	assert_eq!(preview_draft(Some("  ".to_owned())).await.unwrap(), "");
	let html = preview_draft(Some("**Hi**".to_owned())).await.unwrap();
	assert!(html.contains("<strong>Hi</strong>"));

	let rendered = render(move || preview_pane(Ok(html.clone()))).await;
	assert!(rendered.contains("editor-preview book-prose"), "{rendered}");
	let empty = render(|| preview_pane(Ok(String::new()))).await;
	assert!(empty.contains("Nothing to preview yet."), "{empty}");
	let failed = render(|| preview_pane(Err(ServerFnError::new("Too long")))).await;
	assert!(failed.contains("Too long"), "{failed}");
}
