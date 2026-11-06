use super::*;

fn brief() -> PlanningBrief {
	PlanningBrief {
		item: "fb-1".into(),
		version: "brief-v1".into(),
		repository: "acme/web".into(),
		commit: "a".repeat(40),
		text: "Add CSV export".into(),
	}
}

fn plan() -> ImplementationPlan {
	ImplementationPlan {
		summary: "Export invoices".into(),
		acceptance: vec!["CSV includes selected invoices".into()],
		changes: vec!["Add export control and serializer".into()],
		paths: vec!["src/**".into()],
		checks: vec!["Run invoice export tests".into()],
		risks: vec!["Large exports need a limit".into()],
	}
}

#[test]
fn implementation_requires_review_of_the_current_plan() {
	let mut state = PlanningState::default();
	let id = state.queue(brief(), false, 100).unwrap();
	assert!(state.approve(&id, "brief-v1", "owner", 101).is_err());
	let job = state
		.claim("worker", "lease", &["acme/web".into()], 102)
		.unwrap();
	assert_eq!(job.kind, JobKind::Plan);
	state
		.complete_plan(&id, "worker", "lease", plan(), 103)
		.unwrap();
	assert!(state.approve(&id, "brief-v2", "owner", 104).is_err());
	let implementation = state.approve(&id, "brief-v1", "owner", 104).unwrap();

	assert_eq!(state.jobs[1].kind, JobKind::Implement);
	assert_eq!(state.jobs[1].id, implementation);
	assert_eq!(
		state.approve(&id, "brief-v1", "owner", 105).unwrap(),
		implementation
	);
}

#[test]
fn disabled_automatic_planning_cancels_only_unclaimed_automatic_jobs() {
	let mut state = PlanningState::default();
	state.set_policy(true, "acme/web", 0, "owner", 100).unwrap();
	let id = state.queue(brief(), true, 101).unwrap();
	state
		.set_policy(false, "acme/web", 1, "owner", 102)
		.unwrap();
	assert_eq!(state.job(&id).unwrap().status, JobStatus::Cancelled);
	assert!(!state.policy.automatic_features);
}

#[test]
fn stale_leases_and_rejected_plans_cannot_start_implementation() {
	let mut state = PlanningState::default();
	let id = state.queue(brief(), false, 100).unwrap();
	state
		.claim("worker", "lease", &["acme/web".into()], 101)
		.unwrap();
	assert!(
		state
			.complete_plan(&id, "worker", "other", plan(), 102)
			.is_err()
	);
	state
		.complete_plan(&id, "worker", "lease", plan(), 102)
		.unwrap();
	state
		.review(&id, ReviewDecision::Reject, "owner", "Out of scope", 103)
		.unwrap();
	assert!(state.approve(&id, "brief-v1", "owner", 104).is_err());
}

#[test]
fn invalid_plans_and_unapproved_paths_are_rejected() {
	let mut value = plan();
	assert!(value.allows_path("src/export.rs"));
	assert!(!value.allows_path("src-old/export.rs"));
	value.paths = vec!["Cargo.toml".into()];
	assert!(value.allows_path("Cargo.toml"));
	assert!(!value.allows_path("Cargo.toml/child"));
	for path in [
		"",
		"/tmp/file",
		"a/../b",
		".git/config",
		".env.production",
		"src//file",
		"src/*",
		"x\\y",
		"C:file",
		"x\n",
	] {
		assert!(!safe_path(path), "{path:?}");
	}
	value.summary.clear();

	assert!(value.validate().is_err());
	value = plan();
	value.acceptance.clear();
	assert!(value.validate().is_err());
	value = plan();
	value.risks = vec!["x".repeat(2001)];
	assert!(value.validate().is_err());
	value = plan();

	value.paths = vec!["../outside".into()];
	assert!(value.validate().is_err());
}

#[test]
fn queue_deduplicates_pinned_briefs_and_enforces_daily_and_concurrency_limits() {
	let mut state = PlanningState::default();
	assert_eq!(state.job("missing"), Err(PlanningError::NotFound));
	assert!(state.set_policy(true, "invalid", 0, "owner", 1).is_err());
	let mut invalid = brief();
	invalid.commit = "not-a-commit".into();
	assert!(state.queue(invalid, false, 1).is_err());
	assert_eq!(state.queue(brief(), true, 1), Err(PlanningError::Conflict));
	let first = state.queue(brief(), false, 1).unwrap();

	assert_eq!(state.queue(brief(), false, 2).unwrap(), first);
	let mut changed = brief();
	changed.text = "different".into();
	assert_eq!(state.queue(changed, false, 2), Err(PlanningError::Conflict));
	state.policy.daily_limit = 1;
	let mut next = brief();
	next.version = "v2".into();
	assert_eq!(
		state.queue(next.clone(), false, 2),
		Err(PlanningError::Budget)
	);

	state.queue(next, false, 86401).unwrap();
	assert_eq!(state.job(&first).unwrap().status, JobStatus::Superseded);
	assert!(
		state
			.claim("worker", "lease", &["other/repo".into()], 86402)
			.is_none()
	);
	assert!(
		state
			.claim("", "lease", &["acme/web".into()], 86402)
			.is_none()
	);
	state.policy.concurrency = 0;
	assert!(
		state
			.claim("worker", "lease", &["acme/web".into()], 86402)
			.is_none()
	);
}

#[test]
fn cancellation_requires_a_confirmed_stop_and_retries_consume_budget() {
	let mut state = PlanningState::default();
	state.set_policy(true, "acme/web", 0, "owner", 1).unwrap();
	let id = state.queue(brief(), true, 2).unwrap();
	state
		.claim("worker", "lease", &["acme/web".into()], 3)
		.unwrap();
	assert!(!state.heartbeat(&id, "worker", "lease", 4).unwrap());
	assert!(state.retry(&id, "owner", 5).is_err());
	assert!(state.confirm_stopped(&id, "owner", "Stopped", 5).is_err());
	state.cancel(&id).unwrap();

	state.cancel(&id).unwrap();
	assert!(state.heartbeat(&id, "worker", "lease", 6).unwrap());
	assert!(
		state
			.complete_plan(&id, "worker", "lease", plan(), 7)
			.is_err()
	);
	state.finish(&id, "worker", "lease", None, 7).unwrap();
	state.finish(&id, "worker", "lease", None, 8).unwrap();
	assert_eq!(state.job(&id).unwrap().status, JobStatus::Cancelled);
	assert!(state.cancel(&id).is_err());
	state.policy.daily_limit = 1;

	assert_eq!(state.retry(&id, "owner", 9), Err(PlanningError::Budget));
	let retry = state.retry(&id, "owner", 86401).unwrap();
	assert!(!state.job(&retry).unwrap().automatic);
	state
		.claim("worker", "next", &["acme/web".into()], 86402)
		.unwrap();
	assert!(
		state
			.finish(&retry, "worker", "next", Some(&"x".repeat(4001)), 86403)
			.is_err()
	);
	state
		.finish(&retry, "worker", "next", Some("Check failed"), 86403)
		.unwrap();
	state
		.finish(&retry, "worker", "next", Some("Check failed"), 86404)
		.unwrap();
	assert_eq!(state.job(&retry).unwrap().status, JobStatus::Failed);
	assert!(state.retry(&retry, "", 86404).is_err());
}

#[test]
fn expired_jobs_are_fenced_until_the_maintainer_confirms_the_process_stopped() {
	let mut state = PlanningState::default();
	let id = state.queue(brief(), false, 1).unwrap();
	state
		.claim("worker", "lease", &["acme/web".into()], 2)
		.unwrap();
	assert_eq!(
		state.heartbeat(&id, "worker", "lease", 122),
		Err(PlanningError::Lease)
	);
	assert!(state.confirm_stopped(&id, "owner", "", 123).is_err());
	state
		.confirm_stopped(&id, "owner", "Process killed on host", 123)
		.unwrap();
	assert!(
		state
			.complete_plan(&id, "worker", "lease", plan(), 124)
			.is_err()
	);
	assert_eq!(state.job(&id).unwrap().status, JobStatus::Cancelled);

	assert_eq!(state.audit.last().unwrap().actor, "owner");
}

#[test]
fn requested_changes_create_a_new_revision_and_old_results_remain_idempotent() {
	let mut state = PlanningState::default();
	let id = state.queue(brief(), false, 1).unwrap();
	state
		.claim("worker", "lease", &["acme/web".into()], 2)
		.unwrap();
	state
		.complete_plan(&id, "worker", "lease", plan(), 3)
		.unwrap();
	assert!(
		state
			.review(&id, ReviewDecision::RequestChanges, "owner", "", 4)
			.is_err()
	);
	state
		.review(
			&id,
			ReviewDecision::RequestChanges,
			"owner",
			"Include migrations",
			4,
		)
		.unwrap();
	assert!(
		state
			.review(&id, ReviewDecision::Reject, "owner", "No", 5)
			.is_err()
	);
	state
		.complete_plan(&id, "worker", "lease", plan(), 500)
		.unwrap();

	let mut next = brief();
	next.version = "brief-v2".into();
	let revision = state.queue(next, false, 6).unwrap();
	assert_ne!(revision, id);
	assert_eq!(state.job(&id).unwrap().status, JobStatus::Superseded);
	state
		.claim("worker", "new-lease", &["acme/web".into()], 7)
		.unwrap();
	state
		.complete_plan(&revision, "worker", "new-lease", plan(), 8)
		.unwrap();
	let implementation = state.approve(&revision, "brief-v2", "owner", 9).unwrap();

	state
		.complete_plan(&revision, "worker", "new-lease", plan(), 500)
		.unwrap();
	state
		.claim("worker", "implementation-lease", &["acme/web".into()], 10)
		.unwrap();
	assert!(
		state
			.complete_plan(
				&implementation,
				"worker",
				"implementation-lease",
				plan(),
				11
			)
			.is_err()
	);
	state
		.finish(&implementation, "worker", "implementation-lease", None, 12)
		.unwrap();
	assert_eq!(
		state.job(&implementation).unwrap().status,
		JobStatus::Completed
	);
}

#[test]
fn finished_jobs_reject_heartbeats_and_queued_jobs_cancel_immediately() {
	let mut state = PlanningState::default();
	let id = state.queue(brief(), false, 1).unwrap();
	state.cancel(&id).unwrap();
	assert_eq!(state.job(&id).unwrap().status, JobStatus::Cancelled);
	let id = state.retry(&id, "owner", 2).unwrap();
	state
		.claim("worker", "lease", &["acme/web".into()], 3)
		.unwrap();
	state
		.complete_plan(&id, "worker", "lease", plan(), 4)
		.unwrap();
	assert_eq!(
		state.heartbeat(&id, "worker", "lease", 5),
		Err(PlanningError::Lease)
	);
}

#[test]
fn repeating_a_retry_returns_the_same_attempt_without_spending_another_reservation() {
	let mut state = PlanningState::default();
	let id = state.queue(brief(), false, 1).unwrap();
	state.cancel(&id).unwrap();
	let retry = state.retry(&id, "owner", 2).unwrap();
	assert_eq!(state.retry(&id, "owner", 3).unwrap(), retry);
	assert_eq!(state.jobs.len(), 2);
	assert_eq!(
		state.job(&retry).unwrap().retry_of.as_deref(),
		Some(id.as_str())
	);
}
