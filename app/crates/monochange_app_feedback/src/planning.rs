//! Durable planning jobs and explicit approval, independent of worker transport.

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// A failed workflow command never changes persisted state.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanningError {
	#[error("invalid planning input: {0}")]
	Invalid(&'static str),
	#[error("no such planning job")]
	NotFound,
	#[error("the plan or job changed; reload before continuing")]
	Conflict,
	#[error("the worker lease is invalid or expired")]
	Lease,
	#[error("the project planning budget or queue limit has been reached")]
	Budget,
}

/// Project owners explicitly opt into automatic feature planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningPolicy {
	pub automatic_features: bool,
	pub repository: String,
	pub automatic_after: usize,
	pub daily_limit: usize,
	pub concurrency: usize,
}

impl Default for PlanningPolicy {
	fn default() -> Self {
		Self {
			automatic_features: false,
			repository: String::new(),
			automatic_after: 0,
			daily_limit: 10,
			concurrency: 2,
		}
	}
}

/// Only a redacted, pinned brief travels to a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningBrief {
	pub item: String,
	pub version: String,
	pub repository: String,
	pub commit: String,
	pub text: String,
}

/// Reviewed scope is enforced again when implementation files return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImplementationPlan {
	pub summary: String,
	pub acceptance: Vec<String>,
	pub changes: Vec<String>,
	pub paths: Vec<String>,
	pub checks: Vec<String>,
	pub risks: Vec<String>,
}

impl ImplementationPlan {
	/// Reject empty or unbounded model output before it reaches review.
	pub fn validate(&self) -> Result<(), PlanningError> {
		if self.summary.trim().is_empty() || self.summary.len() > 4000 {
			return Err(PlanningError::Invalid("plan summary"));
		}

		for values in [&self.acceptance, &self.changes, &self.paths, &self.checks] {
			if values.is_empty()
				|| values.len() > 20
				|| values
					.iter()
					.any(|value| value.trim().is_empty() || value.len() > 2000)
			{
				return Err(PlanningError::Invalid("plan sections"));
			}
		}

		if self.risks.len() > 20
			|| self.risks.iter().any(|value| value.len() > 2000)
			|| self
				.paths
				.iter()
				.any(|path| !safe_path(path.trim_end_matches("/**")))
		{
			return Err(PlanningError::Invalid("plan paths or risks"));
		}
		Ok(())
	}

	/// A directory scope ends in `/**`; ordinary paths match exactly.
	pub fn allows_path(&self, path: &str) -> bool {
		safe_path(path)
			&& self.paths.iter().any(|scope| {
				scope.strip_suffix("/**").map_or_else(
					|| scope == path,
					|directory| {
						path.strip_prefix(directory)
							.is_some_and(|tail| tail.starts_with('/'))
					},
				)
			})
	}
}

/// Refuse git internals, workflows, secrets, and paths outside the checkout.
pub fn safe_path(path: &str) -> bool {
	!path.is_empty()
		&& path.len() <= 500
		&& !path.starts_with('/')
		&& !path.contains(['\\', ':', '*', '?', '\0', '\n', '\r'])
		&& path.split('/').all(|part| {
			!matches!(
				part,
				"" | "." | ".." | ".git" | ".github" | ".env" | ".monochange"
			)
		}) && !path.split('/').any(|part| part.starts_with(".env."))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
	Plan,
	Implement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
	Queued,
	Running,
	Publishing,
	AwaitingApproval,
	Approved,
	ChangesRequested,
	Rejected,
	CancelRequested,
	Cancelled,
	Failed,
	Completed,
	Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
	Reject,
	RequestChanges,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningJob {
	#[serde(default)]
	pub retry_of: Option<String>,
	pub result_digest: Option<String>,
	pub id: String,
	pub kind: JobKind,
	pub brief: PlanningBrief,
	pub automatic: bool,
	pub status: JobStatus,
	pub created_at: i64,
	pub worker: Option<String>,
	/// Digest only; the lease secret is returned once to the worker.
	pub lease: Option<String>,
	pub lease_until: i64,
	pub plan: Option<ImplementationPlan>,
	pub approved_by: Option<String>,
	pub approved_at: Option<i64>,
	pub implementation: Option<String>,
	pub reason: Option<String>,
	/// The review decision survives superseding a revision so its requested changes stay in the brief.
	#[serde(default)]
	pub review: Option<ReviewDecision>,
	pub pull_request: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningAudit {
	pub actor: String,
	pub action: String,
	pub at: i64,
}

/// Stored alongside feedback so queueing and intake commit atomically.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanningState {
	pub policy: PlanningPolicy,
	pub jobs: Vec<PlanningJob>,
	pub audit: Vec<PlanningAudit>,
}

impl PlanningState {
	pub fn job(&self, id: &str) -> Result<&PlanningJob, PlanningError> {
		self.jobs
			.iter()
			.find(|job| job.id == id)
			.ok_or(PlanningError::NotFound)
	}

	fn job_mut(&mut self, id: &str) -> Result<&mut PlanningJob, PlanningError> {
		self.jobs
			.iter_mut()
			.find(|job| job.id == id)
			.ok_or(PlanningError::NotFound)
	}

	/// Enabling never schedules the existing backlog.
	pub fn set_policy(
		&mut self,
		enabled: bool,
		repository: &str,
		count: usize,
		actor: &str,
		now: i64,
	) -> Result<(), PlanningError> {
		if actor.is_empty() || enabled && repository.split('/').count() != 2 {
			return Err(PlanningError::Invalid("planning repository or actor"));
		}

		if enabled && !self.policy.automatic_features {
			self.policy.automatic_after = count;
		}

		self.policy.automatic_features = enabled;
		repository.clone_into(&mut self.policy.repository);

		if !enabled {
			for job in &mut self.jobs {
				if job.automatic && job.status == JobStatus::Queued {
					job.status = JobStatus::Cancelled;
				}
			}
		}

		self.audit.push(PlanningAudit {
			actor: actor.into(),
			action: format!("automatic planning {enabled}"),
			at: now,
		});
		Ok(())
	}

	/// One job per brief revision, with a durable daily spending reservation.
	pub fn queue(
		&mut self,
		brief: PlanningBrief,
		automatic: bool,
		now: i64,
	) -> Result<String, PlanningError> {
		if brief.text.trim().is_empty()
			|| brief.text.len() > 32_000
			|| brief.item.is_empty()
			|| brief.version.is_empty()
			|| brief.commit.len() != 40
			|| !brief.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
			|| brief.repository.split('/').count() != 2
		{
			return Err(PlanningError::Invalid("planning brief"));
		}
		if let Some(existing) = self.jobs.iter().find(|job| {
			job.kind == JobKind::Plan
				&& job.brief.item == brief.item
				&& job.brief.version == brief.version
				&& !matches!(
					job.status,
					JobStatus::ChangesRequested
						| JobStatus::Cancelled
						| JobStatus::Failed
						| JobStatus::Superseded
				)
		}) {
			if existing.brief != brief {
				return Err(PlanningError::Conflict);
			}

			return Ok(existing.id.clone());
		}

		if automatic && !self.policy.automatic_features {
			return Err(PlanningError::Conflict);
		}

		if self.jobs.len() >= 10_000
			|| self
				.jobs
				.iter()
				.filter(|job| {
					job.kind == JobKind::Plan
						&& job.created_at.div_euclid(86400) == now.div_euclid(86400)
				})
				.count() >= self.policy.daily_limit
		{
			return Err(PlanningError::Budget);
		}

		for old in &mut self.jobs {
			if old.brief.item == brief.item
				&& matches!(
					old.status,
					JobStatus::Queued | JobStatus::AwaitingApproval | JobStatus::ChangesRequested
				) {
				old.status = JobStatus::Superseded;
			}
		}

		let id = format!("job-{}", self.jobs.len() + 1);
		self.jobs.push(PlanningJob {
			retry_of: None,
			result_digest: None,
			id: id.clone(),
			kind: JobKind::Plan,
			brief,
			automatic,
			status: JobStatus::Queued,
			created_at: now,
			worker: None,
			lease: None,
			lease_until: 0,
			plan: None,
			approved_by: None,
			approved_at: None,
			implementation: None,
			reason: None,
			review: None,
			pull_request: None,
		});
		Ok(id)
	}

	/// Claim only authorized repositories. Expired jobs require explicit retry.
	pub fn claim(
		&mut self,
		worker: &str,
		lease: &str,
		repositories: &[String],
		now: i64,
	) -> Option<PlanningJob> {
		if worker.is_empty()
			|| lease.is_empty()
			|| self
				.jobs
				.iter()
				.filter(|job| {
					matches!(
						job.status,
						JobStatus::Running | JobStatus::Publishing | JobStatus::CancelRequested
					)
				})
				.count() >= self.policy.concurrency
		{
			return None;
		}
		let job = self.jobs.iter_mut().find(|job| {
			job.status == JobStatus::Queued && repositories.contains(&job.brief.repository)
		})?;
		job.status = JobStatus::Running;
		job.worker = Some(worker.into());
		job.lease = Some(lease.into());
		job.lease_until = now + 120;
		Some(job.clone())
	}

	fn leased(
		&mut self,
		id: &str,
		worker: &str,
		lease: &str,
		now: i64,
	) -> Result<&mut PlanningJob, PlanningError> {
		let job = self.job_mut(id)?;

		if job.worker.as_deref() != Some(worker)
			|| job.lease.as_deref() != Some(lease)
			|| job.lease_until <= now
			|| !matches!(
				job.status,
				JobStatus::Running | JobStatus::Publishing | JobStatus::CancelRequested
			) {
			return Err(PlanningError::Lease);
		}
		Ok(job)
	}

	/// Workers must stop after cancellation or losing the lease.
	pub fn heartbeat(
		&mut self,
		id: &str,
		worker: &str,
		lease: &str,
		now: i64,
	) -> Result<bool, PlanningError> {
		let job = self.leased(id, worker, lease, now)?;
		job.lease_until = now + 120;
		Ok(job.status == JobStatus::CancelRequested)
	}

	pub fn complete_plan(
		&mut self,
		id: &str,
		worker: &str,
		lease: &str,
		plan: ImplementationPlan,
		now: i64,
	) -> Result<(), PlanningError> {
		plan.validate()?;
		let prior = self.job(id)?;

		if prior.kind == JobKind::Plan
			&& matches!(
				prior.status,
				JobStatus::AwaitingApproval
					| JobStatus::Approved
					| JobStatus::Rejected
					| JobStatus::ChangesRequested
					| JobStatus::Superseded
			) && prior.worker.as_deref() == Some(worker)
			&& prior.lease.as_deref() == Some(lease)
			&& prior.plan.as_ref() == Some(&plan)
		{
			return Ok(());
		}
		let job = self.leased(id, worker, lease, now)?;

		if job.kind != JobKind::Plan || job.status != JobStatus::Running {
			return Err(PlanningError::Conflict);
		}

		job.plan = Some(plan);
		job.status = JobStatus::AwaitingApproval;
		Ok(())
	}

	/// Approval names the immutable revision, and is idempotent under retries.
	pub fn approve(
		&mut self,
		id: &str,
		current_version: &str,
		actor: &str,
		now: i64,
	) -> Result<String, PlanningError> {
		let job = self.job(id)?;

		if job.brief.version != current_version || actor.is_empty() {
			return Err(PlanningError::Conflict);
		}

		if let Some(implementation) = &job.implementation {
			return Ok(implementation.clone());
		}

		if job.status != JobStatus::AwaitingApproval || job.plan.is_none() {
			return Err(PlanningError::Conflict);
		}

		let mut implementation = job.clone();
		implementation.id = format!("job-{}", self.jobs.len() + 1);
		implementation.kind = JobKind::Implement;
		implementation.retry_of = None;
		implementation.status = JobStatus::Queued;
		implementation.automatic = false;
		implementation.created_at = now;
		implementation.worker = None;
		implementation.lease = None;
		implementation.lease_until = 0;
		implementation.approved_by = Some(actor.into());
		implementation.approved_at = Some(now);
		let implementation_id = implementation.id.clone();
		let plan = self.job_mut(id)?;
		plan.status = JobStatus::Approved;
		plan.approved_by = Some(actor.into());
		plan.approved_at = Some(now);
		plan.implementation = Some(implementation_id.clone());
		self.jobs.push(implementation);
		Ok(implementation_id)
	}

	pub fn review(
		&mut self,
		id: &str,
		decision: ReviewDecision,
		actor: &str,
		reason: &str,
		now: i64,
	) -> Result<(), PlanningError> {
		if actor.is_empty() || reason.trim().is_empty() || reason.len() > 2000 {
			return Err(PlanningError::Invalid("review rationale"));
		}

		let job = self.job_mut(id)?;

		if job.status != JobStatus::AwaitingApproval {
			return Err(PlanningError::Conflict);
		}

		job.status = match decision {
			ReviewDecision::Reject => JobStatus::Rejected,
			ReviewDecision::RequestChanges => JobStatus::ChangesRequested,
		};

		job.reason = Some(reason.into());
		job.review = Some(decision);
		self.audit.push(PlanningAudit {
			actor: actor.into(),
			action: format!("{decision:?} {id}: {reason}"),
			at: now,
		});
		Ok(())
	}

	pub fn cancel(&mut self, id: &str) -> Result<(), PlanningError> {
		let job = self.job_mut(id)?;

		job.status = match job.status {
			JobStatus::Queued => JobStatus::Cancelled,
			JobStatus::Running | JobStatus::CancelRequested => JobStatus::CancelRequested,
			_ => return Err(PlanningError::Conflict),
		};

		Ok(())
	}

	pub fn finish(
		&mut self,
		id: &str,
		worker: &str,
		lease: &str,
		failure: Option<&str>,
		now: i64,
	) -> Result<(), PlanningError> {
		if failure.is_some_and(|failure| failure.len() > 4000) {
			return Err(PlanningError::Invalid("worker result"));
		}

		let prior = self.job(id)?;

		if matches!(prior.status, JobStatus::Failed | JobStatus::Cancelled)
			&& prior.worker.as_deref() == Some(worker)
			&& prior.lease.as_deref() == Some(lease)
			&& prior.reason.as_deref() == failure
		{
			return Ok(());
		}
		let job = self.leased(id, worker, lease, now)?;
		job.status = if job.status == JobStatus::CancelRequested {
			JobStatus::Cancelled
		} else if failure.is_some() {
			JobStatus::Failed
		} else {
			JobStatus::Completed
		};

		job.reason = failure.map(str::to_owned);
		Ok(())
	}

	/// Retry only confirmed terminal attempts; a lost worker is not assumed stopped.
	pub fn retry(&mut self, id: &str, actor: &str, now: i64) -> Result<String, PlanningError> {
		let mut job = self.job(id)?.clone();

		if actor.is_empty() || !matches!(job.status, JobStatus::Cancelled | JobStatus::Failed) {
			return Err(PlanningError::Conflict);
		}

		if let Some(retry) = self
			.jobs
			.iter()
			.find(|job| job.retry_of.as_deref() == Some(id))
		{
			return Ok(retry.id.clone());
		}
		job.id = format!("job-{}", self.jobs.len() + 1);
		job.retry_of = Some(id.into());
		job.worker = None;
		job.lease = None;
		job.lease_until = 0;

		job.result_digest = None;

		job.status = JobStatus::Queued;
		job.created_at = now;
		job.reason = None;
		job.automatic = false;

		if job.kind == JobKind::Plan
			&& self
				.jobs
				.iter()
				.filter(|job| {
					job.kind == JobKind::Plan
						&& job.created_at.div_euclid(86400) == now.div_euclid(86400)
				})
				.count() >= self.policy.daily_limit
		{
			return Err(PlanningError::Budget);
		}
		let result = job.id.clone();
		self.jobs.push(job);
		self.audit.push(PlanningAudit {
			actor: actor.into(),
			action: format!("retry {id} as {result}"),
			at: now,
		});
		Ok(result)
	}

	pub fn confirm_stopped(
		&mut self,
		id: &str,
		actor: &str,
		reason: &str,
		now: i64,
	) -> Result<(), PlanningError> {
		let job = self.job_mut(id)?;

		if !matches!(job.status, JobStatus::Running | JobStatus::CancelRequested)
			|| job.lease_until > now
			|| actor.is_empty()
			|| reason.trim().is_empty()
			|| reason.len() > 2000
		{
			return Err(PlanningError::Conflict);
		}
		job.status = JobStatus::Cancelled;
		job.reason = Some(reason.into());
		self.audit.push(PlanningAudit {
			actor: actor.into(),
			action: format!("confirmed stopped {id}: {reason}"),
			at: now,
		});
		Ok(())
	}
}

#[cfg(test)]
#[path = "__tests__/planning_tests.rs"]
mod tests;
