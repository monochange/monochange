//! Install clients, pair workers, and review complete implementation plans.

use leptos::prelude::*;
use monochange_app_feedback::planning::JobStatus;
use monochange_app_feedback::planning::PlanningJob;

use crate::server_fns::embedded::EmbeddedAction;
use crate::server_fns::embedded::EmbeddedReceipt;
use crate::server_fns::embedded::EmbeddedView;
use crate::server_fns::embedded::embedded_console;

#[cfg(test)]
#[path = "__tests__/embedded_feedback_tests.rs"]
mod tests;

#[component]
pub fn EmbeddedFeedbackPanel(organization: String, project: String) -> impl IntoView {
	let action = ServerAction::<EmbeddedAction>::new();
	let organization = StoredValue::new(organization);
	let project = StoredValue::new(project);
	let org = organization.get_value();
	let slug = project.get_value();
	let data = Resource::new_blocking(
		move || action.version().get(),
		move |_| embedded_console(org.clone(), slug.clone()),
	);
	view! {
		<section class="repository-section" aria-labelledby="embedded-feedback-heading">
			<h2 id="embedded-feedback-heading">"Website and app feedback"</h2>
			{move || action_notice(action.value().get())}

			<Suspense fallback=|| view! { <p>"Loading integrations and plans…"</p> }>
				{move || data.get().map(|result| match result {
					Ok(data) => view! { <EmbeddedContent data=data organization=organization.get_value() project=project.get_value() action=action /> }.into_any(),
					Err(error) => view! { <p role="alert">{error.to_string()}</p> }.into_any(),
				})}

			</Suspense>
		</section>
	}
}

fn action_notice(
	result: Option<Result<EmbeddedReceipt, server_fn::ServerFnError>>,
) -> impl IntoView {
	result.map(|result| match result {
  Ok(receipt) => view! { <div role="status"><p>{receipt.message}</p>{receipt.credential.map(|token| view! { <pre class="command-block"><code>{token}</code></pre> })}</div> }.into_any(),
  Err(error) => view! { <p role="alert" class="form-error">{error.to_string()}</p> }.into_any(),
 })
}

#[component]
fn Fields(
	organization: String,
	project: String,
	command: &'static str,
	id: String,
) -> impl IntoView {
	view! { <input type="hidden" name="organization" value=organization /><input type="hidden" name="project" value=project /><input type="hidden" name="action" value=command />{(!id.is_empty()).then(||view!{<input type="hidden" name="id" value=id />})} }
}

#[component]
fn RepositorySelect(repositories: Vec<String>, selected: String) -> impl IntoView {
	view! { <label>"Repository"<select name="repository" required>{repositories.into_iter().map(|repository| { let active = repository == selected; let label = repository.clone(); view! { <option value=repository selected=active>{label}</option> } }).collect::<Vec<_>>()}</select></label> }
}

#[component]
fn EmbeddedContent(
	data: EmbeddedView,
	organization: String,
	project: String,
	action: ServerAction<EmbeddedAction>,
) -> impl IntoView {
	let organization = StoredValue::new(organization);
	let project = StoredValue::new(project);
	let repositories = StoredValue::new(data.repositories.clone());
	let selected = StoredValue::new(data.policy.repository.clone());
	let daily_limit = data.policy.daily_limit;
	let automatic = data.policy.automatic_features;
	let enabled_message = if automatic {
		"New eligible feature requests are planned automatically. Every implementation still needs your approval."
	} else {
		"Choose which requests get a plan. Enable automatic planning to prepare plans for new feature requests."
	};

	view! {
		<p>{format!("Project ID: {}. Install the web, React Native, or Flutter SDK using a client token issued by your backend.",data.project_id)}</p>
		<details><summary>"Register an app"</summary>
			<ActionForm action=action>
				<Fields organization=organization.get_value() project=project.get_value() command="register" id=String::new() />
				<label>"App name"<input name="name" required maxlength="120" /></label>
				<label>"Account namespace"<input name="namespace" required maxlength="120" placeholder="customer-accounts" /></label>
				<p>"Use the same account namespace when web and mobile share user accounts and votes."</p>
				<label>"Surfaces"<select name="surfaces"><option value="both">"Web and mobile"</option><option value="web">"Web"</option><option value="mobile">"Mobile"</option></select></label>
				<label>"Allowed website origins, one per line"<textarea name="origins" rows="3" placeholder="https://app.example.com"></textarea></label>
				<button class="button-primary" type="submit">"Create backend credential"</button>
			</ActionForm>
		</details>
		{data.integrations.into_iter().map(|app| { let app_id = StoredValue::new(app.id.clone()); view! {
			<article class="console-card"><h3>{app.name}</h3><p>{format!("Account namespace: {} · {}",app.namespace,if app.revoked {"Revoked"} else {"Active"})}</p><p>{app.origins.join(", ")}</p>
				{if app.revoked { ().into_any() } else {view! { <div class="actions">
					<ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="rotate" id=app_id.get_value() /><button type="submit">"Rotate credential and expire sessions"</button></ActionForm>
					<ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="revoke_integration" id=app_id.get_value() /><button type="submit">"Revoke app"</button></ActionForm>
				</div> }.into_any()}}
			</article>
		} }).collect::<Vec<_>>()}

		<details><summary>"Connect a local or self-hosted agent"</summary>
			<p>"Create a pairing code for your worker. Codes expire after ten minutes and can be used once."</p>
			<ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="pair_worker" id=String::new() /><label>"Worker name"<input name="name" required maxlength="120" /></label><RepositorySelect repositories=repositories.get_value() selected=String::new() /><button type="submit">"Create pairing code"</button></ActionForm>
		</details>
		{data.workers.into_iter().map(|worker| view! {
			<article class="console-card"><h3>{worker.name}</h3><p>{worker.repositories.join(", ")}</p><p>{format!("{} · Last contact {}",if worker.revoked {"Revoked"} else {"Connected worker"}, worker.last_seen)}</p>
				{(!worker.revoked).then(|| view! { <ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="revoke_worker" id=worker.id /><button type="submit">"Revoke worker"</button></ActionForm> })}
			</article>
		}).collect::<Vec<_>>()}
		<h3>"Planning"</h3><p>{enabled_message}</p>
		<ActionForm action=action>
			<Fields organization=organization.get_value() project=project.get_value() command="policy" id=String::new() />
			<input type="hidden" name="automatic" value=(!automatic).to_string() />
			<RepositorySelect repositories=repositories.get_value() selected=selected.get_value() />
			<label>"Maximum plans per day"<input type="number" name="daily_limit" min="1" max="100" value=daily_limit.to_string() required /></label>

			<button type="submit">{if automatic {"Use manual selection"} else {"Automatically plan feature requests"}}</button>
		</ActionForm>
		<p>"Automatic planning waits for complete triage and skips bugs, held reports, and duplicates. Existing requests are planned only when you select them."</p>
		<ActionForm action=action>
			<Fields organization=organization.get_value() project=project.get_value() command="plan" id=String::new() />
			<label>"Request"<select name="id" required>{data.items.into_iter().map(|(id,title)|view! { <option value=id>{title}</option> }).collect::<Vec<_>>()}</select></label>
			<RepositorySelect repositories=repositories.get_value() selected=selected.get_value() />
			<button type="submit">"Generate plan"</button>
		</ActionForm>
		{(!data.assistant_enabled).then(|| view! { <p>"Conversation assistance is using basic matching. A deployment operator can configure a model provider for semantic suggestions."</p> })}
		{data.jobs.into_iter().map(|job|view! { <JobCard job=job organization=organization.get_value() project=project.get_value() action=action /> }).collect::<Vec<_>>()}
	}
}

#[component]
fn JobCard(
	job: PlanningJob,
	organization: String,
	project: String,
	action: ServerAction<EmbeddedAction>,
) -> impl IntoView {
	let organization = StoredValue::new(organization);
	let project = StoredValue::new(project);
	let job_id = StoredValue::new(job.id.clone());
	let review = job.status == JobStatus::AwaitingApproval;
	let cancellable = matches!(
		job.status,
		JobStatus::Queued | JobStatus::Running | JobStatus::CancelRequested
	);
	let retryable = matches!(job.status, JobStatus::Cancelled | JobStatus::Failed);
	let stopped = matches!(job.status, JobStatus::Running | JobStatus::CancelRequested);
	view! {
		<article class="console-card"><h3>{format!("{} · {} · {:?}",job.brief.item,job.id,job.status)}</h3>
			<p>{format!("{} at {}",job.brief.repository,job.brief.commit)}</p>
			{job.reason.map(|reason|view! { <p>{reason}</p> })}
			{job.plan.map(|plan|view! {
				<h4>{plan.summary}</h4>
				<h5>"Acceptance criteria"</h5><ul>{plan.acceptance.into_iter().map(|value|view! { <li>{value}</li> }).collect::<Vec<_>>()}</ul>
				<h5>"Proposed changes"</h5><ul>{plan.changes.into_iter().map(|value|view! { <li>{value}</li> }).collect::<Vec<_>>()}</ul>
				<h5>"Allowed files"</h5><ul>{plan.paths.into_iter().map(|value|view! { <li><code>{value}</code></li> }).collect::<Vec<_>>()}</ul>
				<h5>"Checks"</h5><ul>{plan.checks.into_iter().map(|value|view! { <li>{value}</li> }).collect::<Vec<_>>()}</ul>
				<h5>"Risks"</h5><ul>{plan.risks.into_iter().map(|value|view! { <li>{value}</li> }).collect::<Vec<_>>()}</ul>
			})}
			{review.then(||view! {
				<p>"Accept the request and create its issue before approving implementation. Approval applies to this exact plan revision."</p>
				<ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="approve" id=job_id.get_value() /><button class="button-primary" type="submit">"Approve and build"</button></ActionForm>
				<ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="changes" id=job_id.get_value() /><label>"Changes needed"<textarea name="rationale" maxlength="2000" required></textarea></label><button type="submit">"Request changes"</button></ActionForm>
				<ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="reject" id=job_id.get_value() /><label>"Reason"<textarea name="rationale" maxlength="2000" required></textarea></label><button type="submit">"Reject plan"</button></ActionForm>
			})}
			{cancellable.then(||view! { <ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="cancel" id=job_id.get_value() /><button type="submit">"Cancel job"</button></ActionForm> })}
			{retryable.then(||view! { <ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="retry" id=job_id.get_value() /><button type="submit">"Retry this approved scope"</button></ActionForm> })}
			{stopped.then(||view! { <details><summary>"Worker disconnected?"</summary><p>"Wait for its lease to expire, stop the agent on its machine, then confirm before retrying."</p><ActionForm action=action><Fields organization=organization.get_value() project=project.get_value() command="confirm_stopped" id=job_id.get_value() /><label>"How you verified the worker stopped"<textarea name="rationale" maxlength="2000" required></textarea></label><button type="submit">"Confirm worker has stopped"</button></ActionForm></details> })}
			{job.pull_request.map(|url|view! { <a class="text-link" href=url>"Review draft pull request"</a> })}
		</article>
	}
}
