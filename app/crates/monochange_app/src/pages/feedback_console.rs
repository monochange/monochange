//! The maintainer console: every feedback item with its triage, what users
//! see, the discussion, and the decisions the pipeline allows.

#[cfg(test)]
#[path = "__tests__/feedback_console_tests.rs"]
mod tests;

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use monochange_app_feedback::Actor;
use monochange_app_feedback::FeedbackItem;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::ReproductionOutcome;
use monochange_app_feedback::Sensitivity;
use monochange_app_feedback::Stage;

use crate::pages::organization::Unavailable;
use crate::pages::organization::action_error;
use crate::server_fns::feedback::ConsoleAction;
use crate::server_fns::feedback::ConsoleItem;
use crate::server_fns::feedback::ConsoleView;
use crate::server_fns::feedback::FeedbackAction;
use crate::server_fns::feedback::MaintainerReply;
use crate::server_fns::feedback::feedback_console;
use crate::server_fns::organizations::organization_path;
use crate::server_fns::organizations::project_path;

/// The stages an item moves through on its way to users, for its progress
/// track.
const TRACK: [Stage; 8] = [
	Stage::Triaging,
	Stage::Discussing,
	Stage::Voting,
	Stage::Accepted,
	Stage::Building,
	Stage::InReview,
	Stage::Merged,
	Stage::Shipped,
];

pub fn stage_label(stage: Stage) -> &'static str {
	match stage {
		Stage::Received => "received",
		Stage::Quarantined => "quarantined",
		Stage::Triaging => "triaging",
		Stage::Discussing => "discussing",
		Stage::Voting => "voting",
		Stage::Accepted => "accepted",
		Stage::Declined => "declined",
		Stage::Building => "building",
		Stage::InReview => "in review",
		Stage::Merged => "merged",
		Stage::Shipped => "shipped",
		Stage::Closed => "closed",
	}
}

fn sensitivity_label(sensitivity: Sensitivity) -> &'static str {
	sensitivity.label()
}

fn plural(count: usize, word: &str) -> String {
	if count == 1 {
		format!("1 {word}")
	} else {
		format!("{count} {word}s")
	}
}

#[component]
pub fn FeedbackConsolePage() -> impl IntoView {
	let params = use_params_map();
	let act = ServerAction::<FeedbackAction>::new();
	let reply = ServerAction::<MaintainerReply>::new();
	let key = move || {
		let params = params.read();
		(
			params.get("organization").unwrap_or_default(),
			params.get("project").unwrap_or_default(),
			act.version().get() + reply.version().get(),
		)
	};
	let console = Resource::new_blocking(key, |(organization, project, _)| {
		feedback_console(organization, project)
	});

	view! {
		<section class="site-width dashboard-section">
			<Suspense fallback=|| view! { <p role="status">"Loading feedback…"</p> }>
				{move || console.get().map(|result| match result {
					Ok(Some(view)) => view! { <ConsoleContent view=view act=act reply=reply /> }.into_any(),
					Ok(None) => view! { <Unavailable title="Project not found" message="It may have been renamed or deleted, or it belongs to an organisation you don't own." /> }.into_any(),
					Err(_) => view! { <Unavailable title="Feedback couldn't be loaded" message="Please reload the page to try again." /> }.into_any(),
				})}
			</Suspense>
		</section>
	}
}

#[component]
fn ConsoleContent(
	view: ConsoleView,
	act: ServerAction<FeedbackAction>,
	reply: ServerAction<MaintainerReply>,
) -> impl IntoView {
	let visibility_note = match view.visibility {
		monochange_app_feedback::RepositoryVisibility::Private => {
			"This project includes a private repository, so users see titles with internal paths, addresses, secrets, and personal details redacted."
		}
		monochange_app_feedback::RepositoryVisibility::Public => {
			"Every repository in this project is public, so users see code paths; internal addresses, secrets, and personal details are still redacted."
		}
	};
	let organization = view.organization.clone();
	let project = view.project_slug.clone();
	let count = view.items.len();
	let others = view.items.clone();

	view! {
		<Title text=format!("Feedback · {} — monochange", view.project_name) />
		<nav class="breadcrumbs" aria-label="Breadcrumb">
			<a href="/dashboard">"Workspace"</a><span aria-hidden="true">"/"</span>
			<a href=organization_path(&organization)>{organization.clone()}</a><span aria-hidden="true">"/"</span>
			<a href=project_path(&organization, &project)>{view.project_name.clone()}</a><span aria-hidden="true">"/"</span>
			<span aria-current="page">"Feedback"</span>
		</nav>
		<div class="project-heading">
			<h1>"Feedback"</h1>
			<p>{visibility_note}</p>
			<div class="actions"><a class="text-link" href=crate::server_fns::portal::portal_path(&organization, &project)>"Open the public portal"</a></div>
		</div>
		{move || act.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
		{move || reply.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
		<section class="repository-section" aria-labelledby="feedback-items-title">
			<div class="repository-heading"><h2 id="feedback-items-title">"Items"</h2><p>{plural(count, "item")}</p></div>
			{if view.items.is_empty() {
				view! { <p class="project-empty">"No feedback yet. Share the public portal link with your users; what they send appears here for triage."</p> }.into_any()
			} else {
				view! {
					<div class="console-list">
						{view.items.into_iter().map(|entry| {
							let others = others.iter().filter(|other| other.item.id != entry.item.id && other.portal_title.is_some() && other.item.stage.is_open()).map(|other| (other.item.id.clone(), other.portal_title.clone().unwrap_or_default())).collect::<Vec<_>>();
							view! {
								<ConsoleCard
									entry=entry
									organization=organization.clone()
									project=project.clone()
									threshold=view.acceptance_threshold
									repositories=view.repositories.clone()
									others=others
									act=act
									reply=reply
								/>
							}
						}).collect::<Vec<_>>()}
					</div>
				}.into_any()
			}}
		</section>
	}
}

/// Hidden fields every action form carries.
#[component]
fn ActionTarget(
	organization: String,
	project: String,
	item: String,
	action: ConsoleAction,
) -> impl IntoView {
	view! {
		<input type="hidden" name="organization" value=organization />
		<input type="hidden" name="project" value=project />
		<input type="hidden" name="item" value=item />
		<input type="hidden" name="action" value=action.as_str() />
	}
}

fn track(stage: Stage) -> impl IntoView {
	let reached = TRACK.iter().position(|step| *step == stage);
	let off = matches!(stage, Stage::Declined | Stage::Closed | Stage::Quarantined);
	view! {
		<span class="stage-track" class:off=off aria-hidden="true">
			{TRACK.iter().enumerate().map(|(index, _)| {
				let done = reached.is_some_and(|reached| index <= reached);
				view! { <i class:done=done></i> }
			}).collect::<Vec<_>>()}
		</span>
	}
}

fn author_name(author: &Actor, submitter: &str) -> String {
	match author {
		Actor::User(id) if id == submitter => format!("{id} (reporter)"),
		Actor::User(id) => id.clone(),
		Actor::Maintainer(login) => format!("@{login}"),
		Actor::Ai => "Assistant".to_owned(),
		Actor::System => "System".to_owned(),
	}
}

fn reproduction(outcome: &ReproductionOutcome) -> AnyView {
	match outcome {
		ReproductionOutcome::Reproduced { steps } => view! {
			<p>"Reproduced"</p>
			<ol class="steps">{steps.iter().map(|step| view! { <li>{step.clone()}</li> }).collect::<Vec<_>>()}</ol>
		}.into_any(),
		ReproductionOutcome::NotReproduced { reasons } => view! { <p>"Not reproduced: " {reasons.join("; ")}</p> }.into_any(),
		ReproductionOutcome::NeedsEnvironment { missing } => view! { <p>"Needs more context: " {missing.join("; ")}</p> }.into_any(),
		ReproductionOutcome::NotApplicable => view! { <p>"Feature request, nothing to reproduce."</p> }.into_any(),
	}
}

fn context_lines(item: &FeedbackItem, threshold: u32) -> Vec<String> {
	let submission = &item.submission;
	let mut lines = vec![match submission.kind {
		FeedbackKind::BugReport => format!("Bug report via {}", submission.app_slug),
		FeedbackKind::FeatureRequest => format!("Feature request via {}", submission.app_slug),
	}];
	match &submission.page {
		Some(page) => {
			lines.push(match &page.app_version {
				Some(version) => format!("Route {} · v{version}", page.route),
				None => format!("Route {}", page.route),
			});
			if let Some(element) = &page.element {
				lines.push(format!(
					"Pinned to {} ({})",
					element.label.clone().unwrap_or_default(),
					element.selector
				));
			}
		}
		None => lines.push("No page context".to_owned()),
	}
	lines.push(format!(
		"{} · {} of {threshold} votes · {}",
		plural(submission.attachments.len(), "attachment"),
		item.votes.total(),
		plural(item.subscribers.len(), "subscriber"),
	));
	if let Some(canonical) = &item.duplicate_of {
		lines.push(format!("Folded into {canonical}"));
	}
	if let Some(decision) = &item.decision {
		let decided = format!(
			"Decided by @{}: {}",
			decision.maintainer, decision.rationale
		);
		lines.push(match &decision.override_vote_threshold {
			Some(override_rationale) => {
				format!("{decided} (threshold overridden: {override_rationale})")
			}
			None => decided,
		});
	}
	if let Some(issue) = &item.issue {
		lines.push(format!(
			"Issue {}#{}",
			issue.repository.clone().unwrap_or_default(),
			issue.number
		));
	}
	lines
}

#[component]
#[allow(clippy::too_many_arguments)]
fn ConsoleCard(
	entry: ConsoleItem,
	organization: String,
	project: String,
	threshold: u32,
	repositories: Vec<String>,
	others: Vec<(String, String)>,
	act: ServerAction<FeedbackAction>,
	reply: ServerAction<MaintainerReply>,
) -> AnyView {
	let item = entry.item.clone();
	let id = item.id.clone();
	let submitter = item.submission.submitter.anonymous_id.clone();
	let title = entry
		.portal_title
		.clone()
		.or_else(|| {
			item.triage
				.as_ref()
				.map(|report| report.product_summary.clone())
		})
		.unwrap_or_else(|| item.submission.description.clone());
	let below_threshold = item.votes.total() < threshold;
	let can_reply = matches!(
		item.stage,
		Stage::Discussing
			| Stage::Voting
			| Stage::Accepted
			| Stage::Building
			| Stage::InReview
			| Stage::Merged
	);
	let reply_fields = view! {
		<input type="hidden" name="organization" value=organization.clone() />
		<input type="hidden" name="project" value=project.clone() />
		<input type="hidden" name="item" value=id.clone() />
	};
	let target = move |action: ConsoleAction| {
		view! { <ActionTarget organization=organization.clone() project=project.clone() item=id.clone() action=action /> }
	};
	let resume_target = target(ConsoleAction::Resume);
	let accept_target = target(ConsoleAction::Accept);
	let open_voting_target = target(ConsoleAction::OpenVoting);
	let create_issue_target = target(ConsoleAction::CreateIssue);
	let duplicate_target = target(ConsoleAction::Duplicate);
	let edit_summary_target = target(ConsoleAction::EditSummary);
	let decline_target = target(ConsoleAction::Decline);
	let close_target = target(ConsoleAction::Close);
	let has = |action: ConsoleAction| entry.actions.contains(&action);
	let suggested: Vec<String> = entry
		.similar
		.iter()
		.map(|similar| similar.id.clone())
		.collect();
	let mut duplicate_targets = others.clone();
	duplicate_targets.sort_by_key(|(other, _)| {
		suggested
			.iter()
			.position(|id| id == other)
			.unwrap_or(usize::MAX)
	});

	view! {
		<details class="console-card" id=format!("item-{}", item.id)>
			<summary>
				<span class="console-id">{item.id.clone()}</span>
				<span class="console-title">{title}</span>
				<span class="stage-badge" data-stage=stage_label(item.stage)>{stage_label(item.stage)}</span>
				{track(item.stage)}
			</summary>
			<div class="console-body">
				<div class="console-block"><h4>"Report from " <code class="console-viewer">{submitter.clone()}</code></h4><p class="console-quote">{item.submission.description.clone()}</p></div>
				<div class="console-block"><h4>"Context"</h4><ul>{context_lines(&item, threshold).into_iter().map(|line| view! { <li>{line}</li> }).collect::<Vec<_>>()}</ul></div>
				{item.triage.clone().map(|report| view! {
					<div class="console-block">
						<h4>"Triage · " {match report.classification { FeedbackKind::BugReport => "bug", FeedbackKind::FeatureRequest => "feature" }}</h4>
						{reproduction(&report.reproduction)}
						{(!report.questions.is_empty()).then(|| view! { <p>"Open questions: " {report.questions.join(" ")}</p> })}
						{(!report.findings.is_empty()).then(|| view! {
							<h4>"Sensitive details found"</h4>
							<ul class="console-findings">{report.findings.iter().map(|finding| view! { <li><code>{finding.detail.clone()}</code><span class="sensitivity">{sensitivity_label(finding.sensitivity)}</span></li> }).collect::<Vec<_>>()}</ul>
						}.into_any())}
					</div>
				}.into_any())}
				<div class="console-block">
					<h4>"What users see"</h4>
					{match (&entry.withheld, &entry.portal_title) {
						(Some(reason), _) => view! { <p class="form-error">"Withheld from users: " {reason.clone()} ". Rewrite its public title."</p> }.into_any(),
						(None, Some(portal_title)) => view! {
							<div class="console-preview">
								<strong>{portal_title.clone()}</strong>
								{(!entry.redacted.is_empty()).then(|| view! { <small>"Hidden: " {entry.redacted.join(", ")}</small> })}
							</div>
						}.into_any(),
						(None, None) => view! { <p class="project-empty">"Not on the public roadmap while " {stage_label(item.stage)} "."</p> }.into_any(),
					}}
				</div>
				{(!item.discussion.is_empty()).then(|| view! {
					<div class="console-block">
						<h4>"Discussion"</h4>
						{item.discussion.iter().map(|message| view! {
							<div class="console-message" class:held=message.held>
								<b>{author_name(&message.author, &submitter)}{message.held.then_some(" · held by screening")}</b>
								<p>{message.body.clone()}</p>
							</div>
						}).collect::<Vec<_>>()}
					</div>
				}.into_any())}
				{(!entry.actions.is_empty()).then(|| view! {
					<div class="console-actions">
						{has(ConsoleAction::Resume).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{resume_target}
								<p>"Screening flagged this text. Vouch for it to let triage run."</p>
								<button type="submit" class="button button-brand">"Vouch and triage"</button>
							</ActionForm>
						}.into_any())}
						{has(ConsoleAction::Accept).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{accept_target}
								<label class="field"><span>"Why accept"</span><input name="rationale" autocomplete="off" /></label>
								{below_threshold.then(|| view! {
									<label class="field"><span>{format!("Only {} of {threshold} votes: why accept anyway?", item.votes.total())}</span><input name="override_rationale" required autocomplete="off" /></label>
								}.into_any())}
								<button type="submit" class="button button-brand">"Accept"</button>
							</ActionForm>
						}.into_any())}
						{has(ConsoleAction::OpenVoting).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{open_voting_target}
								<button type="submit" class="button button-quiet">"Open voting without waiting"</button>
							</ActionForm>
						}.into_any())}
						{has(ConsoleAction::CreateIssue).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{create_issue_target}
								<label class="field"><span>"Open the issue in"</span>
									<select name="repository">{repositories.iter().map(|repository| view! { <option value=repository.clone()>{repository.clone()}</option> }).collect::<Vec<_>>()}</select>
								</label>
								<button type="submit" class="button button-brand">"Create GitHub issue"</button>
							</ActionForm>
						}.into_any())}
						{(has(ConsoleAction::Duplicate) && !duplicate_targets.is_empty()).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{duplicate_target}
								<label class="field"><span>"Fold into"</span>
									<select name="duplicate_of">{duplicate_targets.iter().map(|(other, other_title)| {
										let label = if suggested.contains(other) { format!("Similar · {other} · {other_title}") } else { format!("{other} · {other_title}") };
										view! { <option value=other.clone()>{label}</option> }
									}).collect::<Vec<_>>()}</select>
								</label>
								<button type="submit" class="button button-quiet">"Fold in as duplicate"</button>
							</ActionForm>
						}.into_any())}
						{has(ConsoleAction::EditSummary).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{edit_summary_target}
								<label class="field"><span>"Public title"</span><input name="summary" required autocomplete="off" value=item.summary_override.clone().or_else(|| item.triage.as_ref().map(|report| report.product_summary.clone())).unwrap_or_default() /></label>
								<button type="submit" class="button button-quiet">"Save public title"</button>
							</ActionForm>
						}.into_any())}
						{has(ConsoleAction::Decline).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{decline_target}
								<label class="field"><span>"Why decline " <small>"(shown to users)"</small></span><input name="rationale" required autocomplete="off" /></label>
								<button type="submit" class="button button-danger">"Decline"</button>
							</ActionForm>
						}.into_any())}
						{has(ConsoleAction::Close).then(|| view! {
							<ActionForm action=act attr:class="console-action">
								{close_target}
								<button type="submit" class="text-link">"Close without a decision"</button>
							</ActionForm>
						}.into_any())}
					</div>
				}.into_any())}
				{can_reply.then(|| view! {
					<ActionForm action=reply attr:class="console-reply">
						{reply_fields}
						<label class="field"><span>"Reply to the discussion"</span><textarea name="body" rows="2" required></textarea></label>
						<button type="submit" class="button button-quiet">"Post reply"</button>
					</ActionForm>
				}.into_any())}
				<div class="console-block"><h4>"Timeline"</h4><div class="console-timeline">{item.events.iter().map(|event| view! { <span>{event.command.clone()}</span> }).collect::<Vec<_>>()}</div></div>
			</div>
		</details>
	}
	.into_any()
}
