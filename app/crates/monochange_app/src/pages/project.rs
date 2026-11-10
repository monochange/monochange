//! Element view of one project.

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptos::prelude::RwSignal;
use leptos::prelude::Set;

use crate::server_fns::organizations::ProjectDetails;
use crate::server_fns::organizations::feedback_console_path;
use crate::server_fns::organizations::project_overview;

/// Project screen.
pub fn project_screen(organization: String, project: String) -> leptoskit::prelude::Element {
	let key = crate::slots::once(|| RwSignal::new((organization.clone(), project.clone())));
	key.set((organization, project));
	let details = crate::slots::once(|| {
		Resource::new_blocking(
			move || key.get(),
			|(organization, project)| project_overview(organization, project),
		)
	});

	match details.get() {
		None => crate::ui::text("Loading project…"),
		Some(Ok(Some(details))) => project_content(&details),
		Some(Ok(None)) => crate::ui::heading(1, "Project not found"),
		Some(Err(_)) => crate::ui::heading(1, "This project couldn't be loaded"),
	}
}

fn project_content(details: &ProjectDetails) -> leptoskit::prelude::Element {
	let organization = details.organization.clone();
	let project = details.project.clone();
	let mut children = vec![
		crate::ui::heading(1, project.name.clone()),
		crate::ui::text(project.description.clone()),
	];
	let disconnected = details
		.repositories
		.iter()
		.any(|repository| repository.private.is_none());

	if disconnected {
		children.push(crate::ui::text(
			"Disconnected repositories stay in the project and come back when the GitHub App is installed on them again.",
		));
	}

	for repository in &details.repositories {
		let state = match repository.private {
			Some(true) => "Private",
			Some(false) => "Public",
			None => "Disconnected",
		};
		children.push(crate::ui::text(repository.full_name.clone()));
		children.push(crate::ui::text(state));
	}

	children.push(crate::ui::link(
		feedback_console_path(&organization, &project.slug),
		"Open the feedback console",
	));
	children.push(crate::ui::link(
		crate::server_fns::portal::portal_path(&organization, &project.slug),
		"View the public portal",
	));
	children.push(crate::ui::heading(2, "Edit project"));
	children.push(crate::ui::button("Delete this project"));

	crate::ui::with_children(crate::ui::column("flex flex-col gap-4 p-6"), children)
}
