//! Fixture-backed feedback, planning, approval, and delivery as one lifecycle.

use monochange_app_feedback::planning::ImplementationPlan;
use monochange_app_feedback::planning::JobStatus;
use monochange_app_feedback::planning::PlanningBrief;
use monochange_app_feedback::*;
use serde_json::json;

#[test]
fn host_feedback_reaches_a_reviewed_plan_and_a_shipped_update() {
	let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/feedback-widget/flow");
	let submission: FeedbackSubmission =
		serde_json::from_str(&std::fs::read_to_string(fixtures.join("submission.json")).unwrap())
			.unwrap();
	let plan: ImplementationPlan =
		serde_json::from_str(&std::fs::read_to_string(fixtures.join("plan.json")).unwrap())
			.unwrap();
	let mut service = FeedbackService::new(
		RuleBasedTriage,
		DisclosurePolicy::for_visibility(RepositoryVisibility::Public),
		VotingRules::default(),
		CadenceSnapshot::default(),
	);
	service.register_app(RegisteredApp {
		slug: "example-app".into(),
		display_name: "Example app".into(),
		allowed_origins: vec!["https://example.com".into()],
	});
	let received = service.receive(submission).unwrap();

	service
		.reply(
			&received.id,
			&Actor::User("host-user-pseudonym".into()),
			"Include invoice dates and totals",
			Vec::new(),
		)
		.unwrap();
	service.vote(&received.id, "second-user").unwrap();
	let brief = std::fs::read_to_string(fixtures.join("brief.txt")).unwrap();
	let job = service
		.planning_mut()
		.queue(
			PlanningBrief {
				item: received.id.clone(),
				version: "brief-v1".into(),
				repository: "acme/invoices".into(),
				commit: "a".repeat(40),
				text: brief.clone(),
			},
			false,
			100,
		)
		.unwrap();
	service
		.planning_mut()
		.claim(
			"local-agent",
			"lease-digest",
			&["acme/invoices".into()],
			101,
		)
		.unwrap();
	service
		.planning_mut()
		.complete_plan(&job, "local-agent", "lease-digest", plan.clone(), 102)
		.unwrap();
	assert_eq!(service.planning().jobs.len(), 1);
	assert_eq!(
		service.planning().job(&job).unwrap().status,
		JobStatus::AwaitingApproval
	);

	service
		.accept(
			&received.id,
			MaintainerDecision {
				maintainer: "owner".into(),
				rationale: "Fits the product".into(),
				override_vote_threshold: Some("Requested by customers".into()),
			},
		)
		.unwrap();
	service
		.link_issue(
			&received.id,
			IssueRef {
				repository: Some("acme/invoices".into()),
				number: 41,
				url: Some("https://github.com/acme/invoices/issues/41".into()),
			},
		)
		.unwrap();
	let implementation = service
		.planning_mut()
		.approve(&job, "brief-v1", "owner", 103)
		.unwrap();
	service
		.planning_mut()
		.claim(
			"local-agent",
			"implementation-lease",
			&["acme/invoices".into()],
			104,
		)
		.unwrap();
	service
		.planning_mut()
		.finish(
			&implementation,
			"local-agent",
			"implementation-lease",
			None,
			105,
		)
		.unwrap();
	service.start_build(&received.id).unwrap();
	service
		.open_pull_request(
			&received.id,
			PullRequestRef {
				repository: Some("acme/invoices".into()),
				number: 42,
				url: "https://github.com/acme/invoices/pull/42".into(),
			},
		)
		.unwrap();
	service.observe_merge(Some("acme/invoices"), 42).unwrap();
	service
		.observe_release(&ReleaseObservation {
			repository: Some("acme/invoices".into()),
			version: "1.1.0".into(),
			notes_url: "https://example.com/releases/1.1.0".into(),
			pull_requests: vec![42],
		})
		.unwrap();
	insta::assert_json_snapshot!(
		"feedback_loop",
		json!({"received":received,"plan":plan,"planning_jobs":service.planning().jobs.iter().map(|job|json!({"id":job.id,"kind":job.kind,"status":job.status,"approved_by":job.approved_by})).collect::<Vec<_>>(),"final_stage":service.item("fb-1").unwrap().stage,"feed":service.status_feed().feed})
	);
	insta::assert_snapshot!("planning_brief", brief);
}
