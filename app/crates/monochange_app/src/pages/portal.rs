//! A project's public feedback portal: share bugs and ideas, vote on the
//! roadmap, answer follow-up questions, and follow what happens to them.

#[cfg(test)]
#[path = "__tests__/portal_tests.rs"]
mod tests;

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use leptos_router::hooks::use_query_map;
use monochange_app_feedback::PublicAuthor;
use monochange_app_feedback::PublicLink;
use monochange_app_feedback::PublicStatus;
use monochange_app_feedback::RoadmapEntry;
use monochange_app_feedback::ShipWindow;
use monochange_app_feedback::SimilarItem;

use crate::markdown::MarkdownFormat;
use crate::markdown::apply_format;
use crate::pages::organization::Unavailable;
use crate::pages::organization::action_error;
use crate::server_fns::portal::PortalReply;
use crate::server_fns::portal::PortalSnapshot;
use crate::server_fns::portal::PortalVote;
use crate::server_fns::portal::ShareFeedback;
use crate::server_fns::portal::portal_view;
use crate::server_fns::portal::preview_markdown;
use crate::server_fns::portal::similar_requests;

const STATUS_ORDER: [(PublicStatus, &str); 4] = [
	(PublicStatus::InProgress, "In progress"),
	(PublicStatus::Planned, "Planned"),
	(PublicStatus::UnderReview, "Under review"),
	(PublicStatus::Declined, "Declined"),
];

/// Draws every `[redacted]` token as a redaction bar.
pub fn with_redaction_bars(text: &str) -> Vec<AnyView> {
	let mut parts = Vec::new();
	for (index, piece) in text.split("[redacted]").enumerate() {
		if index > 0 {
			parts.push(
				view! { <span class="redaction" title="Hidden from public view">"redacted"</span> }
					.into_any(),
			);
		}
		if !piece.is_empty() {
			parts.push(piece.to_owned().into_any());
		}
	}
	parts
}

fn ship_window(window: &ShipWindow) -> Option<String> {
	match window {
		ShipWindow::Released { version } => Some(format!("Shipped in v{version}")),
		ShipWindow::NextRelease { label } => {
			Some(match label {
				Some(label) => format!("Ships {label}"),
				None => "Ships in the next release".to_owned(),
			})
		}
		ShipWindow::Planned => Some("Planned · no date yet".to_owned()),
		ShipWindow::Unscheduled => None,
	}
}

fn link_label(link: &PublicLink) -> (&'static str, String) {
	match link {
		PublicLink::Issue(url) => ("Issue", url.clone()),
		PublicLink::PullRequest(url) => ("Pull request", url.clone()),
		PublicLink::ReleaseNotes(url) => ("Release notes", url.clone()),
	}
}

fn author_label(author: PublicAuthor) -> &'static str {
	match author {
		PublicAuthor::Submitter => "Reporter",
		PublicAuthor::Community => "Someone else",
		PublicAuthor::Maintainer => "Team",
		PublicAuthor::Assistant => "Assistant",
	}
}

/// Requests similar to a draft, once it is long enough to compare.
async fn similar_to_draft(
	organization: String,
	project: String,
	text: String,
) -> Result<Vec<SimilarItem>, ServerFnError> {
	if text.trim().chars().count() < 8 {
		return Ok(Vec::new());
	}
	similar_requests(organization, project, text).await
}

/// Points at existing requests a draft resembles, so the visitor can vote
/// instead of filing a duplicate.
fn similar_notice(matches: Vec<SimilarItem>) -> Option<AnyView> {
	(!matches.is_empty()).then(|| {
		view! {
			<div class="portal-similar" aria-live="polite">
				<p>"Looks familiar — you can vote for these on the roadmap instead"</p>
				<ul>{matches.into_iter().map(|item| view! { <li><a href=format!("#item-{}", item.id)>{with_redaction_bars(&item.title)}</a><span class="chip">{format!("{} votes", item.votes)}</span></li> }).collect::<Vec<_>>()}</ul>
			</div>
		}
		.into_any()
	})
}

/// The editor's rendered preview, or nothing for an empty draft.
async fn preview_draft(text: Option<String>) -> Result<String, ServerFnError> {
	match text {
		Some(text) if !text.trim().is_empty() => preview_markdown(text).await,
		_ => Ok(String::new()),
	}
}

fn preview_pane(result: Result<String, ServerFnError>) -> AnyView {
	match result {
		Ok(html) if !html.is_empty() => {
			view! { <div class="editor-preview book-prose" inner_html=html></div> }.into_any()
		}
		Ok(_) => {
			view! { <p class="editor-preview project-empty">"Nothing to preview yet."</p> }
				.into_any()
		}
		Err(error) => view! { <p class="form-error">{action_error(&error)}</p> }.into_any(),
	}
}

#[component]
pub fn PortalPage() -> impl IntoView {
	let params = use_params_map();
	let query = use_query_map();
	let share = ServerAction::<ShareFeedback>::new();
	let vote = ServerAction::<PortalVote>::new();
	let reply = ServerAction::<PortalReply>::new();
	let key = move || {
		let params = params.read();
		(
			params.get("organization").unwrap_or_default(),
			params.get("project").unwrap_or_default(),
			share.version().get() + vote.version().get() + reply.version().get(),
		)
	};
	let snapshot = Resource::new_blocking(key, |(organization, project, _)| {
		portal_view(organization, project)
	});
	let shared = move || query.read().get("shared");

	view! {
		<Suspense fallback=|| view! { <p class="site-width dashboard-section" role="status">"Loading feedback…"</p> }>
			{move || snapshot.get().map(|result| match result {
				Ok(Some(snapshot)) => view! { <PortalContent snapshot=snapshot share=share vote=vote reply=reply shared=shared() /> }.into_any(),
				Ok(None) => view! { <section class="site-width"><Unavailable title="Project not found" message="This feedback portal doesn't exist. Check the link you were given." /></section> }.into_any(),
				Err(_) => view! { <section class="site-width"><Unavailable title="Feedback couldn't be loaded" message="Please reload the page to try again." /></section> }.into_any(),
			})}
		</Suspense>
	}
}

#[component]
fn PortalContent(
	snapshot: PortalSnapshot,
	share: ServerAction<ShareFeedback>,
	vote: ServerAction<PortalVote>,
	reply: ServerAction<PortalReply>,
	shared: Option<String>,
) -> impl IntoView {
	let organization = snapshot.organization.clone();
	let project = snapshot.project_slug.clone();
	let roadmap = snapshot.feed.roadmap.clone();
	let own_items = snapshot.own.clone();
	let groups = STATUS_ORDER
		.iter()
		.filter_map(|(status, heading)| {
			let entries: Vec<RoadmapEntry> = roadmap
				.iter()
				.filter(|entry| entry.status == *status)
				.cloned()
				.collect();
			(!entries.is_empty()).then_some((*heading, entries))
		})
		.collect::<Vec<_>>();
	let has_roadmap = !groups.is_empty();

	view! {
		<Title text=format!("{} feedback — monochange", snapshot.project_name) />
		<section class="portal-hero">
			<div class="site-width">
				<p class="portal-eyebrow">{format!("{organization} · feedback")}</p>
				<h1>{snapshot.project_name.clone()}</h1>
				{(!snapshot.description.is_empty()).then(|| view! { <p>{snapshot.description.clone()}</p> })}
				<nav class="portal-jump" aria-label="Sections"><a href="#share">"Share"</a><a href="#roadmap">"Roadmap"</a><a href="#yours">"Yours"</a></nav>
			</div>
		</section>
		<div class="site-width portal-layout">
			<section id="share" class="portal-panel" aria-labelledby="share-title">
				<h2 id="share-title">"Share a bug or an idea"</h2>
				{shared.map(|id| view! { <p class="portal-thanks" role="status">{format!("Thanks! It's filed as {id}. Follow it under “Yours”.")}</p> })}
				<ShareForm organization=organization.clone() project=project.clone() share=share />
			</section>
			<section id="roadmap" class="portal-panel" aria-labelledby="roadmap-title">
				<h2 id="roadmap-title">"Roadmap"</h2>
				{move || vote.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
				{if has_roadmap {
					view! {
						<div>{groups.into_iter().map(|(heading, entries)| view! {
							<div class="portal-group">
								<h3>{heading}</h3>
								<ul class="portal-list">
									{entries.into_iter().map(|entry| {
										let voted = snapshot.voted.contains(&entry.id);
										let thread = snapshot.threads.get(&entry.id).cloned().unwrap_or_default();
										view! { <RoadmapRow entry=entry voted=voted thread=thread organization=organization.clone() project=project.clone() vote=vote reply=reply /> }
									}).collect::<Vec<_>>()}
								</ul>
							</div>
						}).collect::<Vec<_>>()}</div>
					}.into_any()
				} else {
					view! { <p class="project-empty">"Nothing on the roadmap yet. Be the first to share an idea."</p> }.into_any()
				}}
				{(!snapshot.feed.shipped.is_empty()).then(|| view! {
					<div class="portal-group">
						<h3>"Recently shipped"</h3>
						<ul class="portal-list">
							{snapshot.feed.shipped.iter().map(|entry| view! {
								<li class="portal-item">
									<span class="portal-version">{format!("v{}", entry.version)}</span>
									<div><h4>{with_redaction_bars(&entry.title)}</h4><a class="text-link" href=entry.notes_url.clone() rel="noopener noreferrer">"Release notes"</a></div>
								</li>
							}).collect::<Vec<_>>()}
						</ul>
					</div>
				})}
			</section>
			<section id="yours" class="portal-panel" aria-labelledby="yours-title">
				<h2 id="yours-title">"Yours"</h2>
				{move || reply.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
				{if snapshot.own.is_empty() {
					view! { <p class="project-empty">"What you share appears here, with every update until it ships."</p> }.into_any()
				} else {
					view! {
						<ul class="portal-list">
							{own_items.into_iter().map(|own| view! { <OwnRow own=own organization=organization.clone() project=project.clone() reply=reply /> }).collect::<Vec<_>>()}
						</ul>
					}.into_any()
				}}
				{(!snapshot.updates.is_empty()).then(|| view! {
					<div class="portal-group">
						<h3>"Updates"</h3>
						<ul class="portal-updates">
							{snapshot.updates.iter().map(|notification| view! {
								<li><span class="portal-id">{notification.item_id.clone()}</span><strong>{with_redaction_bars(&notification.update.title)}</strong><span>{with_redaction_bars(&notification.update.body)}</span></li>
							}).collect::<Vec<_>>()}
						</ul>
					</div>
				})}
			</section>
		</div>
	}
}

#[component]
#[allow(clippy::too_many_arguments)]
fn RoadmapRow(
	entry: RoadmapEntry,
	voted: bool,
	thread: Vec<monochange_app_feedback::PublicMessage>,
	organization: String,
	project: String,
	vote: ServerAction<PortalVote>,
	reply: ServerAction<PortalReply>,
) -> impl IntoView {
	let votable = entry.status != PublicStatus::Declined
		&& !matches!(
			entry.ship_window,
			ShipWindow::NextRelease { .. } | ShipWindow::Released { .. }
		);
	let discussable = entry.status != PublicStatus::Declined;
	let kind = match entry.kind {
		monochange_app_feedback::FeedbackKind::BugReport => "Bug",
		monochange_app_feedback::FeedbackKind::FeatureRequest => "Idea",
	};
	let vote_label = if voted {
		format!("Remove your vote ({})", entry.votes)
	} else {
		format!("Vote ({})", entry.votes)
	};
	let anchor = format!("item-{}", entry.id);
	let thread_view = (discussable || !thread.is_empty()).then(|| {
		view! { <Thread thread=thread item=entry.id.clone() organization=organization.clone() project=project.clone() reply=reply discussable=discussable /> }
	});

	view! {
		<li class="portal-item" id=anchor>
			<ActionForm action=vote attr:class="portal-vote">
				<input type="hidden" name="organization" value=organization.clone() />
				<input type="hidden" name="project" value=project.clone() />
				<input type="hidden" name="item" value=entry.id.clone() />
				<input type="hidden" name="on" value=(!voted).to_string() />
				<button type="submit" aria-pressed=voted.to_string() disabled=!votable aria-label=vote_label>{entry.votes}</button>
			</ActionForm>
			<div class="portal-item-body">
				<h4>{with_redaction_bars(&entry.title)}</h4>
				<div class="portal-meta">
					<span>{with_redaction_bars(&entry.status_line)}</span>
					<span class="chip">{kind}</span>
					{ship_window(&entry.ship_window).map(|label| view! { <span class="chip">{label}</span> })}
					{entry.links.iter().map(|link| { let (label, url) = link_label(link); view! { <a class="text-link" href=url rel="noopener noreferrer">{label}</a> } }).collect::<Vec<_>>()}
				</div>
				{thread_view}
			</div>
		</li>
	}
}

#[component]
fn Thread(
	thread: Vec<monochange_app_feedback::PublicMessage>,
	item: String,
	organization: String,
	project: String,
	reply: ServerAction<PortalReply>,
	discussable: bool,
) -> impl IntoView {
	view! {
		<details class="portal-thread">
			<summary>{format!("Discussion ({})", thread.len())}</summary>
			{thread.iter().map(|message| view! {
				<div class="console-message"><b>{author_label(message.author)}</b><p>{with_redaction_bars(&message.body)}</p></div>
			}).collect::<Vec<_>>()}
			{discussable.then(|| view! {
				<ActionForm action=reply attr:class="portal-reply">
					<input type="hidden" name="organization" value=organization />
					<input type="hidden" name="project" value=project />
					<input type="hidden" name="item" value=item />
					<label class="field"><span>"Add to the discussion"</span><textarea name="body" rows="2" required></textarea></label>
					<button type="submit" class="button button-quiet">"Comment"</button>
				</ActionForm>
			})}
		</details>
	}
}

#[component]
fn OwnRow(
	own: crate::server_fns::portal::OwnItem,
	organization: String,
	project: String,
	reply: ServerAction<PortalReply>,
) -> impl IntoView {
	let anchor = format!("item-{}", own.id);
	let awaiting = own.question.is_some();
	let item_id = own.id.clone();
	let question = own.question.clone().map(|question| {
		view! {
			<div class="portal-question"><b>"We have a question"</b><p>{with_redaction_bars(&question)}</p></div>
			<ActionForm action=reply attr:class="portal-reply">
				<input type="hidden" name="organization" value=organization />
				<input type="hidden" name="project" value=project />
				<input type="hidden" name="item" value=item_id />
				<label class="field"><span>"Your answer"</span><textarea name="body" rows="3" required></textarea></label>
				<button type="submit" class="button button-brand">"Answer"</button>
			</ActionForm>
		}
	});
	view! {
		<li class="portal-own" class:awaiting=awaiting id=anchor>
			<span class="portal-id">{own.id.clone()}</span>
			<h4>{with_redaction_bars(&own.update.title)}</h4>
			<p class="portal-status">{with_redaction_bars(&own.update.body)}</p>
			{question}
		</li>
	}
}

#[component]
fn ShareForm(
	organization: String,
	project: String,
	share: ServerAction<ShareFeedback>,
) -> impl IntoView {
	let (text, set_text) = signal(String::new());
	let similar = {
		let organization = organization.clone();
		let project = project.clone();
		Resource::new(
			move || text.get(),
			move |text| similar_to_draft(organization.clone(), project.clone(), text),
		)
	};

	view! {
		<ActionForm action=share attr:class="portal-form">
			<input type="hidden" name="organization" value=organization />
			<input type="hidden" name="project" value=project />
			<fieldset class="portal-kind">
				<legend class="sr-only">"What are you sharing?"</legend>
				<label><input type="radio" name="kind" value="bug_report" checked /><span>"Something's wrong"</span></label>
				<label><input type="radio" name="kind" value="feature_request" /><span>"I have an idea"</span></label>
			</fieldset>
			<MarkdownEditor name="description" label="Tell us more" text=text set_text=set_text />
			<Suspense fallback=|| ()>
				{move || similar.get().and_then(Result::ok).and_then(similar_notice)}
			</Suspense>
			{move || share.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
			<button type="submit" class="button button-brand" disabled=move || share.pending().get()>{move || if share.pending().get() { "Sending…" } else { "Send feedback" }}</button>
		</ActionForm>
	}
}

/// A Markdown textarea with a formatting toolbar and a preview. The textarea
/// is a normal form field, so the form works before hydration; the toolbar
/// and preview need the page to be interactive. It isn't `required`: while
/// previewing it is hidden, and a hidden required field would block the form
/// silently, so the server's validation message reports an empty one.
#[component]
pub fn MarkdownEditor(
	name: &'static str,
	label: &'static str,
	text: ReadSignal<String>,
	set_text: WriteSignal<String>,
) -> impl IntoView {
	let textarea = NodeRef::<leptos::html::Textarea>::new();
	let (previewing, set_previewing) = signal(false);
	let preview = Resource::new(move || previewing.get().then(|| text.get()), preview_draft);
	// patch-coverage:ignore-start -- Toolbar clicks only run in the browser on a hydrated textarea; the edit itself is `apply_format`, covered by the markdown tests.
	let format = move |format: MarkdownFormat| {
		let Some(element) = textarea.get() else {
			return;
		};
		let start = element.selection_start().ok().flatten().unwrap_or(0);
		let end = element.selection_end().ok().flatten().unwrap_or(start);
		let result = apply_format(&element.value(), start, end, format);
		element.set_value(&result.text);
		set_text.set(result.text);
		let _ = element.focus();
		let _ = element.set_selection_range(result.selection_start, result.selection_end);
	};
	// patch-coverage:ignore-end

	view! {
		<div class="editor">
			<div class="editor-bar">
				<span class="editor-label" id=format!("{name}-label")>{label}</span>
				<div class="editor-tabs" role="tablist">
					<button type="button" role="tab" aria-selected=move || (!previewing.get()).to_string() on:click=move |_| set_previewing.set(false)>"Write"</button>
					<button type="button" role="tab" aria-selected=move || previewing.get().to_string() on:click=move |_| set_previewing.set(true)>"Preview"</button>
				</div>
			</div>
			<div class="editor-toolbar" role="toolbar" aria-label="Formatting" hidden=move || previewing.get()>
				{MarkdownFormat::TOOLBAR.into_iter().map(|action| view! {
					<button type="button" title=action.label() aria-label=action.label() on:click=move |_| format(action)>{action.glyph()}</button>
				}).collect::<Vec<_>>()}
			</div>
			<textarea
				node_ref=textarea
				name=name
				rows="7"
				maxlength=monochange_app_feedback::submission::MAX_DESCRIPTION_CHARS
				aria-labelledby=format!("{name}-label")
				placeholder="What happened, or what would you love to see? Markdown works: **bold**, lists, `code`."
				hidden=move || previewing.get()
				on:input=move |event| set_text.set(event_target_value(&event))
			></textarea>
			<Suspense fallback=|| view! { <p class="editor-preview" role="status">"Rendering preview…"</p> }>
				{move || previewing.get().then(|| preview.get().map(preview_pane))}
			</Suspense>
		</div>
	}
}
