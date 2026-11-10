//! Element view of a project's public feedback portal.

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptos::prelude::RwSignal;
use leptos::prelude::Set;
use monochange_app_feedback::PublicLink;
use monochange_app_feedback::PublicStatus;
use monochange_app_feedback::RoadmapEntry;
use monochange_app_feedback::ShipWindow;

use crate::server_fns::portal::PortalSnapshot;
use crate::server_fns::portal::portal_view;

const STATUS_ORDER: [(PublicStatus, &str); 4] = [
	(PublicStatus::InProgress, "In progress"),
	(PublicStatus::Planned, "Planned"),
	(PublicStatus::UnderReview, "Under review"),
	(PublicStatus::Declined, "Declined"),
];

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

/// Public feedback portal.
pub fn portal_screen(
	organization: String,
	project: String,
	shared: Option<String>,
) -> leptoskit::prelude::Element {
	let key = crate::slots::once(|| RwSignal::new((organization.clone(), project.clone())));
	key.set((organization, project));
	let snapshot = crate::slots::once(|| {
		Resource::new_blocking(
			move || key.get(),
			|(organization, project)| portal_view(organization, project),
		)
	});

	match snapshot.get() {
		None => crate::ui::text("Loading the portal…"),
		Some(Ok(Some(snapshot))) => portal_content(&snapshot, shared.as_deref()),
		Some(Ok(None)) => crate::ui::heading(1, "This feedback portal doesn't exist"),
		Some(Err(_)) => crate::ui::heading(1, "Feedback couldn't be loaded"),
	}
}

fn portal_content(snapshot: &PortalSnapshot, shared: Option<&str>) -> leptoskit::prelude::Element {
	crate::ui::with_children(
		crate::ui::column("flex flex-col gap-3 p-6"),
		portal_lines(snapshot, shared).into_iter().map(|line| {
			if let Some(href) = line.strip_prefix("href:") {
				let (href, label) = href.split_once('|').unwrap_or((href, href));
				crate::ui::link(href, label)
			} else {
				crate::ui::text(line)
			}
		}),
	)
}

fn portal_lines(snapshot: &PortalSnapshot, shared: Option<&str>) -> Vec<String> {
	let mut lines = vec!["Share a bug or an idea".to_owned()];

	if let Some(id) = shared {
		lines.push(format!(
			"Thanks! It's filed as {id}. Follow it under “Yours”."
		));
	}

	for own in &snapshot.own {
		lines.push(monochange_app_feedback::roadmap::public_status_line(own.stage).to_owned());

		if own.question.is_some() {
			lines.push("We have a question".to_owned());
			lines.push("Your answer".to_owned());
		}
	}

	if !snapshot.updates.is_empty() {
		lines.push("Updates".to_owned());
	}

	if snapshot.viewer.is_none() {
		lines.push("Continue with GitHub".to_owned());
		lines.push(
			"Sign in to follow what you share, from the first reply to the release that ships it."
				.to_owned(),
		);
	}

	for (status, heading) in STATUS_ORDER {
		let entries: Vec<&RoadmapEntry> = snapshot
			.feed
			.roadmap
			.iter()
			.filter(|entry| entry.status == status)
			.collect();

		if entries.is_empty() {
			continue;
		}

		lines.push(heading.to_owned());

		for entry in entries {
			lines.push(shown_text(&entry.title));

			if let Some(window) = ship_window(&entry.ship_window) {
				lines.push(window);
			}

			if snapshot.viewer.is_none() {
				lines.push(format!("Sign in with GitHub to vote ({})", entry.votes));
			}

			for link in &entry.links {
				let (label, href) = link_label(link);
				lines.push(format!("href:{href}|{label}"));
			}
		}
	}

	if !snapshot.feed.shipped.is_empty() {
		lines.push("Recently shipped".to_owned());

		for entry in &snapshot.feed.shipped {
			lines.push(shown_text(&entry.title));
			lines.push(format!("v{}", entry.version));
			lines.push(format!("href:{}|Release notes", entry.notes_url));
		}
	}

	lines
}

/// Replaces each `[redacted]` token with the word visitors can see.
fn shown_text(text: &str) -> String {
	text.replace("[redacted]", "redacted")
}
