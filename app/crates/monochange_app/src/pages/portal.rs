//! A project's public feedback portal: share bugs and ideas, vote on the
//! roadmap, answer follow-up questions, and follow what happens to them.

#[cfg(test)]
#[path = "__tests__/portal_tests.rs"]
mod tests;

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use leptos_router::hooks::use_query_map;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::PublicAuthor;
use monochange_app_feedback::PublicLink;
use monochange_app_feedback::PublicStatus;
use monochange_app_feedback::RoadmapEntry;
use monochange_app_feedback::ShipWindow;
use monochange_app_feedback::SimilarItem;
use monochange_app_feedback::Stage;

use crate::components::arrow::ArrowIcon;
use crate::markdown::MarkdownFormat;
use crate::markdown::apply_format;
use crate::pages::organization::Unavailable;
use crate::pages::organization::action_error;
use crate::server_fns::portal::PortalReply;
use crate::server_fns::portal::PortalSnapshot;
use crate::server_fns::portal::PortalVote;
use crate::server_fns::portal::ShareFeedback;
use crate::server_fns::portal::portal_sign_in_path;
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

/// The class that styles a discussion message by who wrote it.
fn author_class(author: PublicAuthor) -> &'static str {
	match author {
		PublicAuthor::Submitter => "submitter",
		PublicAuthor::Community => "community",
		PublicAuthor::Maintainer => "team",
		PublicAuthor::Assistant => "assistant",
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

/// The steps a visitor's own item moves through, in order.
const TRACKER_STEPS: [&str; 5] = ["Shared", "Reviewed", "Planned", "Building", "Shipped"];

/// How far along the tracker `stage` is, or `None` once the item was
/// declined or closed and will not move again.
fn tracker_step(stage: Stage) -> Option<usize> {
	match stage {
		Stage::Received | Stage::Quarantined | Stage::Triaging | Stage::Discussing => Some(0),
		Stage::Voting => Some(1),
		Stage::Accepted => Some(2),
		Stage::Building | Stage::InReview | Stage::Merged => Some(3),
		Stage::Shipped => Some(4),
		Stage::Declined | Stage::Closed => None,
	}
}

/// The class that colours a roadmap group's marker.
fn status_class(status: PublicStatus) -> &'static str {
	match status {
		PublicStatus::InProgress => "in-progress",
		PublicStatus::Planned => "planned",
		PublicStatus::UnderReview => "under-review",
		PublicStatus::Shipped => "shipped",
		PublicStatus::Declined => "declined",
	}
}

fn count(roadmap: &[RoadmapEntry], status: PublicStatus) -> usize {
	roadmap
		.iter()
		.filter(|entry| entry.status == status)
		.count()
}

#[component]
fn GitHubMark() -> impl IntoView {
	view! {
		<svg class="github-mark" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" /></svg>
	}
}

/// A bug or a lightbulb, matching what was shared.
#[component]
fn KindIcon(kind: FeedbackKind) -> impl IntoView {
	let path = match kind {
		FeedbackKind::BugReport => {
			"M8 9a4 4 0 1 1 8 0v5a4 4 0 0 1-8 0V9Zm4-4V3M9 4l1.5 2M15 4l-1.5 2M4 10h4m8 0h4M4 15h4m8 0h4M12 13v6"
		}
		FeedbackKind::FeatureRequest => {
			"M9 18h6m-5 3h4M12 3a6 6 0 0 0-3.6 10.8c.6.5 1 1.2 1 2V16h5.2v-.2c0-.8.4-1.5 1-2A6 6 0 0 0 12 3Z"
		}
	};
	view! {
		<svg class="kind-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d=path /></svg>
	}
}

/// Asks a signed-out visitor to continue with GitHub, and comes back to
/// `anchor` afterwards.
#[component]
fn SignInButton(href: String, #[prop(into)] label: String, class: &'static str) -> impl IntoView {
	view! { <a href=href class=format!("button {class}")><GitHubMark />{label}</a> }
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
	let signed_in = snapshot.viewer.is_some();
	let sign_in = |anchor: &str| portal_sign_in_path(&organization, &project, anchor);
	let roadmap = snapshot.feed.roadmap.clone();
	let groups = STATUS_ORDER
		.iter()
		.filter_map(|(status, heading)| {
			let entries: Vec<RoadmapEntry> = roadmap
				.iter()
				.filter(|entry| entry.status == *status)
				.cloned()
				.collect();
			(!entries.is_empty()).then_some((*status, *heading, entries))
		})
		.collect::<Vec<_>>();
	let has_roadmap = !groups.is_empty();
	let stats = [
		("Under review", count(&roadmap, PublicStatus::UnderReview)),
		("Planned", count(&roadmap, PublicStatus::Planned)),
		("In progress", count(&roadmap, PublicStatus::InProgress)),
		("Shipped", snapshot.feed.shipped.len()),
	];
	let identity = match snapshot.viewer.clone() {
		Some(viewer) => {
			view! {
				<div class="portal-viewer">
					{viewer.avatar_url.map(|url| view! { <img src=url alt="" width="32" height="32" /> })}
					<span><b>{format!("@{}", viewer.login)}</b><small>"Your name stays private here"</small></span>
				</div>
			}
			.into_any()
		}
		None => view! { <SignInButton href=sign_in("share") label="Sign in with GitHub" class="button-light portal-hero-sign-in" /> }.into_any(),
	};

	view! {
		<Title text=format!("{} feedback — monochange", snapshot.project_name) />
		<section class="portal-hero">
			<div class="site-width">
				<div class="portal-hero-top">
					<p class="portal-eyebrow">{format!("{organization} · Feedback")}</p>
					{identity}
				</div>
				<h1>{snapshot.project_name.clone()}</h1>
				{(!snapshot.description.is_empty()).then(|| view! { <p class="portal-lede">{snapshot.description.clone()}</p> })}
				<dl class="portal-stats">
					{stats.into_iter().map(|(label, value)| view! { <div><dt>{label}</dt><dd>{value}</dd></div> }).collect::<Vec<_>>()}
				</dl>
				<nav class="portal-jump" aria-label="Sections"><a href="#share">"Share"</a><a href="#roadmap">"Roadmap"</a><a href="#yours">"Yours"</a></nav>
			</div>
		</section>
		<div class="site-width portal-layout">
			<section id="share" class="portal-panel portal-share" aria-labelledby="share-title">
				<h2 id="share-title">"Share a bug or an idea"</h2>
				{shared.map(|id| view! { <p class="portal-thanks" role="status"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12.5 4.5 4.5L19 7.5" /></svg>{format!("Thanks! It's filed as {id}. Follow it under “Yours”.")}</p> })}
				{if signed_in {
					view! { <ShareForm organization=organization.clone() project=project.clone() share=share /> }.into_any()
				} else {
					view! {
						<div class="portal-gate">
							<div class="portal-gate-icons" aria-hidden="true"><span><KindIcon kind=FeedbackKind::BugReport /></span><span><KindIcon kind=FeedbackKind::FeatureRequest /></span></div>
							<p>"Sign in with GitHub to share, vote, and hear about every step until it ships. One account counts once, and your GitHub name isn't shown on this page."</p>
							<SignInButton href=sign_in("share") label="Continue with GitHub" class="button-brand" />
						</div>
					}.into_any()
				}}
			</section>
			<section id="roadmap" class="portal-panel portal-roadmap" aria-labelledby="roadmap-title">
				<div class="portal-panel-head">
					<h2 id="roadmap-title">"Roadmap"</h2>
					<p>"Vote for what you want next. The team decides what to build, and every step shows here."</p>
				</div>
				{move || vote.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}

				{if has_roadmap {
					view! {
						<div class="portal-groups">{groups.into_iter().map(|(status, heading, entries)| view! {
							<div class="portal-group" data-status=status_class(status)>
								<h3><span class="portal-dot" aria-hidden="true"></span>{heading}<span class="portal-count">{entries.len()}</span></h3>
								<ul class="portal-list">
									{entries.into_iter().map(|entry| {
										let voted = snapshot.voted.contains(&entry.id);
										let thread = snapshot.threads.get(&entry.id).cloned().unwrap_or_default();
										let sign_in = sign_in(&format!("item-{}", entry.id));
										view! { <RoadmapRow entry=entry voted=voted thread=thread organization=organization.clone() project=project.clone() vote=vote reply=reply sign_in=(!signed_in).then_some(sign_in) /> }
									}).collect::<Vec<_>>()}
								</ul>
							</div>
						}).collect::<Vec<_>>()}</div>
					}.into_any()
				} else {
					view! { <div class="portal-empty"><KindIcon kind=FeedbackKind::FeatureRequest /><p>"Nothing on the roadmap yet. Be the first to share an idea."</p></div> }.into_any()
				}}
				{(!snapshot.feed.shipped.is_empty()).then(|| view! {
					<div class="portal-group" data-status="shipped">
						<h3><span class="portal-dot" aria-hidden="true"></span>"Recently shipped"<span class="portal-count">{snapshot.feed.shipped.len()}</span></h3>
						<ol class="portal-timeline">
							{snapshot.feed.shipped.iter().map(|entry| view! {
								<li>
									<span class="portal-version">{format!("v{}", entry.version)}</span>
									<div><h4>{with_redaction_bars(&entry.title)}</h4><a class="text-link" href=entry.notes_url.clone() rel="noopener noreferrer">"Release notes"<ArrowIcon /></a></div>
								</li>
							}).collect::<Vec<_>>()}
						</ol>
					</div>
				})}

			</section>
			<section id="yours" class="portal-panel portal-yours" aria-labelledby="yours-title">
				<h2 id="yours-title">"Yours"</h2>
				{move || reply.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}

				{if !signed_in {
					view! {
						<div class="portal-gate portal-gate-quiet">
							<p>"Sign in to follow what you share, from the first reply to the release that ships it."</p>
							<SignInButton href=sign_in("yours") label="Sign in with GitHub" class="button-quiet" />
						</div>
					}.into_any()
				} else if snapshot.own.is_empty() {
					view! { <p class="project-empty">"What you share appears here, with every update until it ships."</p> }.into_any()
				} else {
					view! {
						<ul class="portal-own-list">
							{snapshot.own.iter().cloned().map(|own| view! { <OwnRow own=own organization=organization.clone() project=project.clone() reply=reply /> }).collect::<Vec<_>>()}
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
	/// Where a signed-out visitor goes to sign in; `None` once signed in.
	sign_in: Option<String>,
) -> impl IntoView {
	let votable = entry.status != PublicStatus::Declined
		&& !matches!(
			entry.ship_window,
			ShipWindow::NextRelease { .. } | ShipWindow::Released { .. }
		);
	let discussable = entry.status != PublicStatus::Declined;

	let kind = match entry.kind {
		FeedbackKind::BugReport => "Bug",
		FeedbackKind::FeatureRequest => "Idea",
	};
	let chevron = view! { <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 14 6-6 6 6" /></svg> };
	let vote_control = match &sign_in {
		Some(href) if votable => {
			view! {
				<a class="portal-vote-button" href=href.clone() aria-label=format!("Sign in with GitHub to vote ({})", entry.votes)>{chevron}<span>{entry.votes}</span></a>
			}
			.into_any()
		}
		_ => {
			let vote_label = if voted {
				format!("Remove your vote ({})", entry.votes)
			} else {
				format!("Vote ({})", entry.votes)
			};
			let (organization, project, item) =
				(organization.clone(), project.clone(), entry.id.clone());
			view! {
				<ActionForm action=vote attr:class="portal-vote">
					<input type="hidden" name="organization" value=organization />
					<input type="hidden" name="project" value=project />
					<input type="hidden" name="item" value=item />
					<input type="hidden" name="on" value=(!voted).to_string() />
					<button type="submit" class="portal-vote-button" aria-pressed=voted.to_string() disabled=!votable aria-label=vote_label>{chevron}<span>{entry.votes}</span></button>
				</ActionForm>
			}
			.into_any()
		}
	};

	let anchor = format!("item-{}", entry.id);
	let thread_view = (discussable || !thread.is_empty()).then(|| {
		view! { <Thread thread=thread item=entry.id.clone() organization=organization.clone() project=project.clone() reply=reply discussable=discussable sign_in=sign_in.clone() /> }
	});

	view! {
		<li class="portal-card" id=anchor>
			{vote_control}
			<div class="portal-card-body">
				<div class="portal-card-tags">
					<span class="portal-kind-tag"><KindIcon kind=entry.kind />{kind}</span>
					{ship_window(&entry.ship_window).map(|label| view! { <span class="portal-ship">{label}</span> })}
				</div>
				<h4>{with_redaction_bars(&entry.title)}</h4>
				<p class="portal-card-status">{with_redaction_bars(&entry.status_line)}</p>
				{(!entry.links.is_empty()).then(|| view! {
					<div class="portal-links">
						{entry.links.iter().map(|link| { let (label, url) = link_label(link); view! { <a href=url rel="noopener noreferrer">{label}</a> } }).collect::<Vec<_>>()}
					</div>
				})}
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
	sign_in: Option<String>,
) -> impl IntoView {
	let summary = match thread.len() {
		0 => "Start the discussion".to_owned(),
		1 => "1 comment".to_owned(),
		count => format!("{count} comments"),
	};
	let composer = discussable.then(|| match sign_in {
		Some(href) => view! { <a class="text-link portal-thread-sign-in" href=href>"Sign in with GitHub to join the discussion"</a> }.into_any(),
		None => {
			view! {
				<ActionForm action=reply attr:class="portal-reply">
					<input type="hidden" name="organization" value=organization />
					<input type="hidden" name="project" value=project />
					<input type="hidden" name="item" value=item />
					<label class="field"><span class="sr-only">"Add to the discussion"</span><textarea name="body" rows="2" required placeholder="Add to the discussion"></textarea></label>
					<button type="submit" class="button button-quiet">"Comment"</button>
				</ActionForm>
			}
			.into_any()
		}
	});
	view! {
		<details class="portal-thread">
			<summary><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 5h16v11H9l-5 4V5Z" /></svg>{summary}</summary>
			<div class="portal-messages">
				{thread.iter().map(|message| view! {
					<div class="portal-message" data-author=author_class(message.author)><b>{author_label(message.author)}</b><p>{with_redaction_bars(&message.body)}</p></div>
				}).collect::<Vec<_>>()}
			</div>
			{composer}
		</details>
	}
}

/// The tracker under one of the visitor's items.
fn tracker(stage: Stage) -> AnyView {
	let Some(current) = tracker_step(stage) else {
		return view! { <p class="portal-closed">{monochange_app_feedback::roadmap::public_status_line(stage)}</p> }.into_any();
	};

	view! {
		<ol class="portal-tracker" aria-label="Progress">
			{TRACKER_STEPS.iter().enumerate().map(|(index, step)| {
				let progress = match index.cmp(&current) {
					std::cmp::Ordering::Less => "done",
					std::cmp::Ordering::Equal => "current",
					std::cmp::Ordering::Greater => "next",
				};
				view! { <li data-state=progress aria-current=(index == current).then_some("step")><span>{*step}</span></li> }
			}).collect::<Vec<_>>()}
		</ol>
	}
	.into_any()
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
			<div class="portal-own-head"><span class="portal-id">{own.id.clone()}</span>{awaiting.then(|| view! { <span class="portal-needs-you">"Needs your answer"</span> })}</div>
			<h4>{with_redaction_bars(&own.update.title)}</h4>
			<p class="portal-status">{with_redaction_bars(&own.update.body)}</p>
			{tracker(own.stage)}
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
				<label><input type="radio" name="kind" value="bug_report" checked /><span><KindIcon kind=FeedbackKind::BugReport /><b>"Something's wrong"</b><small>"Report a bug"</small></span></label>
				<label><input type="radio" name="kind" value="feature_request" /><span><KindIcon kind=FeedbackKind::FeatureRequest /><b>"I have an idea"</b><small>"Suggest a feature"</small></span></label>
			</fieldset>
			<MarkdownEditor name="description" label="Tell us more" text=text set_text=set_text />
			<Suspense fallback=|| ()>
				{move || similar.get().and_then(Result::ok).and_then(similar_notice)}
			</Suspense>
			{move || share.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
			<div class="portal-form-foot">
				<p>"The team reads everything. You'll see each step under “Yours”."</p>
				<button type="submit" class="button button-brand" disabled=move || share.pending().get()>{move || if share.pending().get() { "Sending…" } else { "Send feedback" }}</button>
			</div>
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
