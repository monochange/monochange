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
use crate::server_fns::portal::PortalViewer;
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
		viewer: Some(PortalViewer {
			login: "carol".to_owned(),
			avatar_url: Some("https://avatars.example/carol".to_owned()),
		}),
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
		"@carol",
		"Your name stays private here",
		"src=\"https://avatars.example/carol\"",
		"Needs your answer",
		"4 comments",
		"aria-current=\"step\"",
		"Send feedback",
	] {
		assert!(html.contains(expected), "missing {expected}: {html}");
	}
	assert!(!html.contains("Nothing on the roadmap yet"));
	assert!(!html.contains("Continue with GitHub"));
	assert!(!html.contains("/login?next="));
}

#[tokio::test]
async fn signed_out_visitors_read_everything_and_are_invited_to_sign_in() {
	let mut snapshot = snapshot(StatusFeed {
		roadmap: vec![
			entry("fb-1", PublicStatus::UnderReview, ShipWindow::Planned),
			entry("fb-2", PublicStatus::InProgress, ShipWindow::Unscheduled),
			entry("fb-3", PublicStatus::Declined, ShipWindow::Unscheduled),
		],
		shipped: Vec::new(),
	});
	snapshot.viewer = None;
	snapshot.description = "Invoicing".to_owned();
	let html = render_portal(snapshot, None).await;

	for expected in [
		"Invoicing",
		"Continue with GitHub",
		"Sign in with GitHub to vote (5)",
		"Sign in with GitHub to join the discussion",
		"Sign in to follow what you share",
		"href=\"/login?next=%2Fp%2Falice%2Fpocketbook%23share\"",
		"href=\"/login?next=%2Fp%2Falice%2Fpocketbook%23item-fb-1\"",
		"href=\"/login?next=%2Fp%2Falice%2Fpocketbook%23yours\"",
		"Start the discussion",
		"<dt>Under review</dt><dd>1</dd>",
		"<dt>In progress</dt><dd>1</dd>",
		"<dt>Shipped</dt><dd>0</dd>",
	] {
		assert!(html.contains(expected), "missing {expected}: {html}");
	}

	// Nothing that writes is offered without an account, and a declined item
	// can't be voted on even after signing in.
	assert!(!html.contains("Send feedback"));
	assert!(!html.contains("name=\"body\""));
	assert_eq!(
		html.matches("aria-label=\"Sign in with GitHub to vote")
			.count(),
		2
	);
	assert!(html.contains("aria-label=\"Vote (5)\""));
}

#[tokio::test]
async fn the_tracker_follows_an_item_until_it_ships_or_stops() {
	assert_eq!(tracker_step(Stage::Received), Some(0));
	assert_eq!(tracker_step(Stage::Quarantined), Some(0));
	assert_eq!(tracker_step(Stage::Triaging), Some(0));
	assert_eq!(tracker_step(Stage::Discussing), Some(0));
	assert_eq!(tracker_step(Stage::Voting), Some(1));
	assert_eq!(tracker_step(Stage::Accepted), Some(2));
	assert_eq!(tracker_step(Stage::Building), Some(3));
	assert_eq!(tracker_step(Stage::InReview), Some(3));
	assert_eq!(tracker_step(Stage::Merged), Some(3));
	assert_eq!(tracker_step(Stage::Shipped), Some(4));
	assert_eq!(tracker_step(Stage::Declined), None);
	assert_eq!(tracker_step(Stage::Closed), None);

	let building = render(|| tracker(Stage::InReview)).await;
	assert_eq!(building.matches("data-state=\"done\"").count(), 3);
	assert!(building.contains("data-state=\"current\" aria-current=\"step\"><span>Building"));
	assert_eq!(building.matches("data-state=\"next\"").count(), 1);
	let declined = render(|| tracker(Stage::Declined)).await;
	assert!(declined.contains("portal-closed"), "{declined}");
	assert!(declined.contains("Declined"));
	assert!(!declined.contains("portal-tracker"));

	for (status, class) in [
		(PublicStatus::InProgress, "in-progress"),
		(PublicStatus::Planned, "planned"),
		(PublicStatus::UnderReview, "under-review"),
		(PublicStatus::Shipped, "shipped"),
		(PublicStatus::Declined, "declined"),
	] {
		assert_eq!(status_class(status), class);
	}

	for (author, class) in [
		(PublicAuthor::Submitter, "submitter"),
		(PublicAuthor::Community, "community"),
		(PublicAuthor::Maintainer, "team"),
		(PublicAuthor::Assistant, "assistant"),
	] {
		assert_eq!(author_class(author), class);
	}
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
	let (owner, _) = context(&state, None);
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
