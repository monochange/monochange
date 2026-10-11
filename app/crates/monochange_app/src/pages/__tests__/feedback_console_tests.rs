//! The console explains every item state a maintainer can meet.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use monochange_app_feedback::DiscussionMessage;
use monochange_app_feedback::FeedbackSubmission;
use monochange_app_feedback::MaintainerDecision;
use monochange_app_feedback::PageContext;
use monochange_app_feedback::RepositoryVisibility;
use monochange_app_feedback::SubmitterIdentity;
use monochange_app_feedback::TriageReport;
use monochange_app_feedback::VotingRules;

use super::*;
use crate::server_fns::feedback::console_actions;
use crate::tests::render;

fn console(visibility: RepositoryVisibility, items: Vec<ConsoleItem>) -> ConsoleView {
	ConsoleView {
		organization: "alice".to_owned(),
		project_name: "Pocketbook".to_owned(),
		project_slug: "pocketbook".to_owned(),
		visibility,
		acceptance_threshold: 3,
		repositories: vec!["alice/web".to_owned(), "alice/api".to_owned()],
		items,
	}
}

async fn render_console(view: ConsoleView) -> String {
	Box::pin(render(move || {
		view! { <ConsoleContent view=view.clone() act=ServerAction::new() reply=ServerAction::new() /> }
	}))
	.await
}

/// An accepted bug that triage couldn't reproduce, decided without an
/// override, still waiting for its issue.
fn accepted_bug() -> ConsoleItem {
	let mut item = FeedbackItem::new(
		"fb-1".to_owned(),
		FeedbackSubmission {
			kind: FeedbackKind::BugReport,
			description: "Totals are wrong".to_owned(),
			page: Some(PageContext {
				route: "/invoices".to_owned(),
				app_version: None,
				locale: None,
				element: None,
			}),
			attachments: Vec::new(),
			submitter: SubmitterIdentity {
				anonymous_id: "v-1".to_owned(),
				email: None,
			},
			app_slug: "portal".to_owned(),
		},
		VotingRules::default(),
	);
	item.stage = Stage::Accepted;
	item.triage = Some(TriageReport {
		classification: FeedbackKind::BugReport,
		reproduction: ReproductionOutcome::NotReproduced {
			reasons: vec!["Totals matched".to_owned()],
		},
		questions: Vec::new(),
		findings: Vec::new(),
		product_summary: "Invoice totals are wrong".to_owned(),
	});
	item.decision = Some(MaintainerDecision {
		maintainer: "alice".to_owned(),
		rationale: "Confirmed by support".to_owned(),
		override_vote_threshold: None,
	});
	item.discussion.push(DiscussionMessage {
		author: Actor::System,
		body: "Moved to accepted".to_owned(),
		attachments: Vec::new(),
		held: false,
	});
	let actions = console_actions(&item);
	ConsoleItem {
		item,
		portal_title: Some("Invoice totals are wrong".to_owned()),
		redacted: Vec::new(),
		withheld: None,
		actions,
		similar: Vec::new(),
	}
}

#[test]
fn every_stage_has_a_label() {
	let labels = [
		Stage::Received,
		Stage::Quarantined,
		Stage::Triaging,
		Stage::Discussing,
		Stage::Voting,
		Stage::Accepted,
		Stage::Declined,
		Stage::Building,
		Stage::InReview,
		Stage::Merged,
		Stage::Shipped,
		Stage::Closed,
	]
	.map(stage_label);
	assert_eq!(
		labels,
		[
			"received",
			"quarantined",
			"triaging",
			"discussing",
			"voting",
			"accepted",
			"declined",
			"building",
			"in review",
			"merged",
			"shipped",
			"closed",
		]
	);
}

#[tokio::test]
async fn an_empty_public_project_explains_what_users_see() {
	let html = render_console(console(RepositoryVisibility::Public, Vec::new())).await;
	assert!(
		html.contains("Every repository in this project is public"),
		"{html}"
	);
	assert!(html.contains("No feedback yet."), "{html}");
}

#[tokio::test]
async fn accepted_items_offer_an_issue_in_any_connected_repository() {
	let entry = accepted_bug();
	assert!(entry.actions.contains(&ConsoleAction::CreateIssue));
	let html = render_console(console(RepositoryVisibility::Private, vec![entry])).await;
	for expected in [
		"Create GitHub issue",
		"<option value=\"alice/web\">",
		"<option value=\"alice/api\">",
		"Not reproduced: ",
		"Totals matched",
		"Route /invoices",
		"Decided by @alice: Confirmed by support",
		"System",
	] {
		assert!(html.contains(expected), "missing {expected}: {html}");
	}
	assert!(!html.contains("threshold overridden"));
}
