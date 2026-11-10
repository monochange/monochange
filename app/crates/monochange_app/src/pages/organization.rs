//! One organisation: its projects and its connected repositories.

use leptos::prelude::*;
use leptoskit::native::Role;
use leptoskit::prelude::Checkbox;
use leptoskit::prelude::Element;
use leptoskit::prelude::Text;
use leptoskit::prelude::View;

use crate::pages::dashboard::organization_avatar;
use crate::repository_filter::repository_page;
use crate::server_fns::organizations::OrganizationSummary;
use crate::server_fns::organizations::organization_overview;
use crate::server_fns::organizations::organization_path;
use crate::server_fns::organizations::project_path;
use crate::ui::heading;
use crate::ui::link;
use crate::ui::text;

/// The message to show for a failed server action.
pub fn action_error(error: &server_fn::ServerFnError) -> String {
	match error {
		server_fn::ServerFnError::ServerError(message) => message.clone(),
		other => other.to_string(),
	}
}

/// Account screen for `/dashboard/:organization`.
pub fn organization_screen(login: String, search: Option<String>) -> Element {
	#[derive(Clone, Copy)]
	struct LoginSlot(RwSignal<String>);

	let slot = crate::slots::once(|| LoginSlot(RwSignal::new(login.clone())));
	slot.0.set(login);
	let overview =
		crate::slots::once(|| Resource::new_blocking(move || slot.0.get(), organization_overview));
	let search = search.unwrap_or_default();

	match overview.get() {
		None => text("Loading organisation…"),
		Some(Ok(Some(organization))) => organization_content(organization, &search),
		Some(Ok(None)) => {
			unavailable(
				"Organisation not found",
				"It isn't connected, or you're not one of its owners.",
			)
		}
		Some(Err(_)) => {
			unavailable(
				"This organisation couldn't be loaded",
				"Please reload the page to try again.",
			)
		}
	}
}

/// Leptos wrapper for pages that still embed the status view.
#[leptos::prelude::component]
pub fn Unavailable(title: &'static str, message: &'static str) -> impl IntoView {
	leptoskit::web::render(unavailable(title, message))
}

/// A not-found or failure state with a way back.
pub fn unavailable(title: &'static str, message: &'static str) -> Element {
	View()
		.class("flex flex-col gap-4")
		.child(heading(1, title))
		.child(text(message))
		.child(link("/dashboard", "Back to your workspace"))
}

/// The account body. Tests render this with a fixture instead of the server.
pub fn organization_content(organization: OrganizationSummary, search: &str) -> Element {
	let kind = if organization.personal {
		"Personal account"
	} else {
		"GitHub organisation"
	};
	let login = organization.login.clone();
	let total = organization.repositories.len();
	let page = repository_page(&organization.repositories, search, 1);
	let count = if search.trim().is_empty() {
		format!("{total} connected")
	} else {
		format!("{} of {total} match", page.matches)
	};

	let mut column = View()
		.class("flex flex-col gap-4")
		.child(link("/dashboard", "Workspace"))
		.child(Text(login.clone()).aria_current(leptoskit::native::Current::Page))
		.child(organization_avatar(&login, organization.avatar_url.clone()))
		.child(heading(1, login.clone()))
		.child(Text(format!(
			"{kind} · {} connected repositories",
			organization.repositories.len()
		)))
		.child(heading(2, "Projects"))
		.child(text("Group repositories that ship one product."));

	if organization.projects.is_empty() {
		column = column.child(text(
			"No projects yet. A project brings several of this organisation's repositories together under one name.",
		));
	} else {
		for project in &organization.projects {
			let href = project_path(&login, &project.slug);
			let count = match project.repository_count {
				1 => "1 repository".to_owned(),
				count => format!("{count} repositories"),
			};
			let detail = if project.description.is_empty() {
				count.clone()
			} else {
				project.description.clone()
			};
			column = column
				.child(link(href, &project.name))
				.child(Text(detail))
				.child(Text(count));
		}
	}

	column = if organization.supports_projects {
		let example = project_path(&login, "invoices");
		column
			.child(text("New project"))
			.child(text("Name"))
			.child(Text(format!(
				"Its address comes from the name: “Invoices” lives at {example}"
			)))
			.child(text("Description (optional)"))
			.child(text("Create project"))
			.child(repository_picker(&organization.repositories))
	} else {
		column.child(text(
			"monochange is still confirming this account with GitHub. Reload the page in a moment to create projects.",
		))
	};

	column
		.child(heading(2, "Repositories"))
		.child(Text(count))
		.child(link(organization_path(&login), "Search repositories"))
		.child(repository_list(&page))
}

fn repository_picker(repositories: &[crate::server_fns::repos::RepoInfo]) -> Element {
	let mut picker = View().class("flex flex-col gap-2");
	if repositories.len() > crate::components::repository_list::PICKER_FILTER_THRESHOLD {
		picker = picker.child(text("Filter repositories"));
	}
	for repository in repositories {
		picker = picker
			.child(Checkbox(false).aria_label(repository.github_full_name.clone()))
			.child(Text(repository.github_full_name.clone()));
	}
	picker
}

fn repository_list(page: &crate::repository_filter::RepositoryPage) -> Element {
	if page.matches == 0 {
		return text(format!("No repositories match “{}”.", "the current search"));
	}

	let mut list = View().role(Role::List).class("flex flex-col gap-2");
	for repository in &page.repositories {
		let visibility = if repository.github_private {
			"Private"
		} else {
			"Public"
		};
		let mut row = View()
			.role(Role::ListItem)
			.child(Text(repository.github_full_name.clone()))
			.child(text(visibility));
		if repository.installation_suspended {
			row = row.child(text("Installation suspended"));
		}
		list = list.child(row);
	}

	if page.page_count > 1 {
		list = list.child(Text(format!("Page {} of {}", page.page, page.page_count)));
	}
	list
}

#[cfg(test)]
#[path = "__tests__/organization_tests.rs"]
mod tests;
