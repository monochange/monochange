//! Element view of the maintainer feedback console.

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptos::prelude::RwSignal;
use leptos::prelude::Set;
use monochange_app_feedback::Actor;
use monochange_app_feedback::FeedbackItem;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::ReproductionOutcome;
use monochange_app_feedback::Stage;

use crate::server_fns::feedback::ConsoleAction;
use crate::server_fns::feedback::ConsoleView;
use crate::server_fns::feedback::feedback_console;

/// Maintainer feedback console.
pub fn console_screen(organization: String, project: String) -> leptoskit::prelude::Element {
	let key = crate::slots::once(|| RwSignal::new((organization.clone(), project.clone())));
	key.set((organization, project));
	let console = crate::slots::once(|| {
		Resource::new_blocking(
			move || key.get(),
			|(organization, project)| feedback_console(organization, project),
		)
	});

	match console.get() {
		None => crate::ui::text("Loading feedback…"),
		Some(Ok(Some(view))) => console_content(&view),
		Some(Ok(None)) => crate::ui::heading(1, "Project not found"),
		Some(Err(_)) => crate::ui::heading(1, "Feedback couldn't be loaded"),
	}
}

fn console_content(view: &ConsoleView) -> leptoskit::prelude::Element {
	crate::ui::with_children(
		crate::ui::column("flex flex-col gap-2 p-6"),
		console_lines(view).into_iter().map(crate::ui::text),
	)
}

fn stage_label(stage: Stage) -> &'static str {
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

fn plural(count: usize, word: &str) -> String {
	if count == 1 {
		format!("1 {word}")
	} else {
		format!("{count} {word}s")
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

	if item.stage == Stage::Building {
		lines.push("An agent is building this; its pull request moves it to review".to_owned());
	}

	lines
}

fn console_lines(view: &ConsoleView) -> Vec<String> {
	let visibility_note = match view.visibility {
		monochange_app_feedback::RepositoryVisibility::Private => {
			"This project includes a private repository, so users see titles with internal paths, addresses, secrets, and personal details redacted."
		}
		monochange_app_feedback::RepositoryVisibility::Public => {
			"Every repository in this project is public, so users see code paths; internal addresses, secrets, and personal details are still redacted."
		}
	};
	let mut lines = vec![
		"Feedback".to_owned(),
		visibility_note.to_owned(),
		"Open the public portal".to_owned(),
	];

	for entry in &view.items {
		let item = &entry.item;
		let submitter = item.submission.submitter.anonymous_id.clone();
		lines.push(format!("Report from {submitter}"));
		lines.extend(context_lines(item, view.acceptance_threshold));

		if let Some(report) = &item.triage {
			match &report.reproduction {
				ReproductionOutcome::Reproduced { steps } => lines.extend(steps.clone()),
				ReproductionOutcome::NotReproduced { reasons } => {
					lines.push(format!("Not reproduced: {}", reasons.join("; ")));
				}
				ReproductionOutcome::NeedsEnvironment { missing } => {
					lines.push(format!("Needs more context: {}", missing.join("; ")));
				}
				ReproductionOutcome::NotApplicable => {
					lines.push("Feature request, nothing to reproduce.".to_owned());
				}
			}

			if !report.findings.is_empty() {
				lines.push("Sensitive details found".to_owned());
				lines.extend(report.findings.iter().map(|finding| finding.detail.clone()));
			}
		}

		match (&entry.withheld, &entry.portal_title) {
			(Some(reason), _) => {
				lines.push(format!(
					"Withheld from users: {reason}. Rewrite its public title."
				));
			}
			(None, Some(_)) if !entry.redacted.is_empty() => {
				lines.push(format!("Hidden: {}", entry.redacted.join(", ")));
			}
			(None, None) => {
				lines.push(format!(
					"Not on the public roadmap while {}.",
					stage_label(item.stage)
				));
			}
			(None, Some(_)) => {}
		}

		for message in &item.discussion {
			if message.held {
				lines.push(format!(
					"{} · held by screening",
					author_name(&message.author, &submitter)
				));
			}
		}

		if entry.actions.contains(&ConsoleAction::Resume) {
			lines.push("Screening flagged this text. Vouch for it to let triage run.".to_owned());
		}

		let can_reply = matches!(
			item.stage,
			Stage::Discussing
				| Stage::Voting
				| Stage::Accepted
				| Stage::Building
				| Stage::InReview
				| Stage::Merged
		);
		if can_reply {
			lines.push("Reply to the discussion".to_owned());
		}

		for similar in &entry.similar {
			lines.push(format!("Similar · {}", similar.id));
		}
	}

	lines
}

#[cfg(test)]
#[path = "__tests__/feedback_console_tests.rs"]
mod tests;
