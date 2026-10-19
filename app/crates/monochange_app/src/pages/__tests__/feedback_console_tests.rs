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
		shared_compute: false,
		repositories: vec!["alice/web".to_owned(), "alice/api".to_owned()],
		items,
	}
}

async fn render_console(view: ConsoleView) -> String {
	Box::pin(render(move || {
		view! { <ConsoleContent view=view.clone() act=ServerAction::new() reply=ServerAction::new() tasks=TaskActions::new() /> }
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
		tasks: Vec::new(),
		shareable: false,
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

#[tokio::test]
async fn issued_items_can_be_built_by_the_repository_agent() {
	let mut entry = accepted_bug();
	entry.item.issue = Some(monochange_app_feedback::IssueRef {
		repository: Some("alice/web".to_owned()),
		number: 9,
		url: None,
	});
	entry.actions = console_actions(&entry.item);
	let html = render_console(console(RepositoryVisibility::Private, vec![entry.clone()])).await;
	for expected in [
		"Build with an agent",
		"value=\"build_with_agent\"",
		"Runs the agent workflow in <code>alice/web</code>",
		"never merges on its own",
	] {
		assert!(html.contains(expected), "missing {expected}: {html}");
	}

	entry.item.stage = Stage::Building;
	entry.actions = console_actions(&entry.item);
	let html = render_console(console(RepositoryVisibility::Private, vec![entry])).await;
	assert!(
		html.contains("An agent is building this; its pull request moves it to review"),
		"{html}"
	);
	assert!(!html.contains("Build with an agent"), "{html}");
}

#[tokio::test]
async fn only_public_projects_can_turn_on_shared_compute() {
	let html = render_console(console(RepositoryVisibility::Private, Vec::new())).await;
	assert!(
		html.contains("Shared compute needs every repository in this project to be public"),
		"{html}"
	);
	assert!(!html.contains("Turn on"), "{html}");

	let html = render_console(console(RepositoryVisibility::Public, Vec::new())).await;
	assert!(html.contains("Experimental"), "{html}");
	assert!(html.contains("Turn on"), "{html}");
	assert!(html.contains("name=\"enabled\" value=\"true\""), "{html}");

	let mut enabled = console(RepositoryVisibility::Public, Vec::new());
	enabled.shared_compute = true;
	let html = render_console(enabled).await;
	assert!(html.contains("Turn off"), "{html}");
	assert!(html.contains("name=\"enabled\" value=\"false\""), "{html}");
	// A project that turned private can still turn it off.
	let mut private = console(RepositoryVisibility::Private, Vec::new());
	private.shared_compute = true;
	assert!(render_console(private).await.contains("Turn off"));
}

fn shared(id: &str, status: TaskStatus, open: bool) -> ConsoleTask {
	ConsoleTask {
		task: monochange_app_feedback::ContributorTask {
			id: id.to_owned(),
			item: "fb-1".to_owned(),
			target: monochange_app_feedback::TaskTarget {
				repository: "alice/web".to_owned(),
				commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
				issue_number: 9,
			},
			draft: monochange_app_feedback::TaskDraft {
				title: format!("Round totals {id}"),
				acceptance: vec!["Totals match".to_owned()],
				paths: vec!["src/totals/**".to_owned()],
				check: "cargo test totals".to_owned(),
			},
			shared_by: "alice".to_owned(),
			created_at: 0,
			expires_at: 0,
			status,
		},
		open,
		expires: "2 November 2026".to_owned(),
	}
}

#[tokio::test]
async fn shared_tasks_show_their_state_and_screening_reasons() {
	let mut entry = accepted_bug();
	entry.shareable = true;
	entry.tasks = vec![
		shared(
			"task-4",
			TaskStatus::Rejected {
				reasons: vec![monochange_app_feedback::TaskRejection::UnsupportedCheck {
					reason: "can't be empty".to_owned(),
				}],
			},
			false,
		),
		shared("task-3", TaskStatus::Withdrawn, false),
		shared("task-2", TaskStatus::Listed, false),
		shared("task-1", TaskStatus::Listed, true),
	];
	let html = render_console(console(RepositoryVisibility::Public, vec![entry])).await;
	for expected in [
		"Contributor tasks",
		"task-4 · Rejected by screening · cargo test totals",
		"the check command can't be empty",
		"task-3 · Withdrawn",
		"task-2 · No longer listed",
		"task-1 · Listed until 2 November 2026",
		"Share with contributors",
		"name=\"acceptance\"",
		"value=\"Invoice totals are wrong\"",
		"name=\"task\" value=\"task-1\"",
	] {
		assert!(html.contains(expected), "missing {expected}: {html}");
	}
	// A withdrawn task can't be withdrawn again.
	assert!(!html.contains("name=\"task\" value=\"task-3\""), "{html}");
}

#[tokio::test]
async fn items_without_tasks_or_a_share_form_show_no_task_block() {
	let html = render_console(console(RepositoryVisibility::Public, vec![accepted_bug()])).await;
	assert!(!html.contains("Contributor tasks"), "{html}");

	let mut entry = accepted_bug();
	entry.tasks = vec![shared("task-1", TaskStatus::Withdrawn, false)];
	let html = render_console(console(RepositoryVisibility::Public, vec![entry])).await;
	assert!(html.contains("Contributor tasks"), "{html}");
	assert!(!html.contains("Share with contributors"), "{html}");
}
