//! One organisation: its projects and its connected repositories.

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptos::prelude::RwSignal;
use leptoskit::native::Role;
use leptoskit::prelude::*;

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
pub fn Unavailable(title: &'static str, message: &'static str) -> impl leptos::prelude::IntoView {
	leptoskit::web::render(unavailable(title, message))
}

/// A not-found or failure state with a way back.
pub fn unavailable(title: &'static str, message: &'static str) -> Element {
	let title = heading(1, title);
	let message = text(message);
	let back = link("/dashboard", "Back to your workspace");
	leptoskit::view! {
		<View class="flex flex-col gap-4">
			{title}
			{message}
			{back}
		</View>
	}
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

	let current = login.clone();
	let mut children = vec![
		link("/dashboard", "Workspace"),
		leptoskit::view! { <Text aria_current=leptoskit::native::Current::Page>{current}</Text> },
		organization_avatar(&login, organization.avatar_url.clone()),
		heading(1, login.clone()),
		leptoskit::view! {
			<Text>
				{format!(
					"{kind} · {} connected repositories",
					organization.repositories.len()
				)}
			</Text>
		},
		heading(2, "Projects"),
		text("Group repositories that ship one product."),
	];

	if organization.projects.is_empty() {
		children.push(text(
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
			children.push(link(href, &project.name));
			children.push(leptoskit::view! { <Text>{detail}</Text> });
			children.push(leptoskit::view! { <Text>{count}</Text> });
		}
	}

	if organization.supports_projects {
		let example = project_path(&login, "invoices");
		children.push(text("New project"));
		children.push(text("Name"));
		children.push(leptoskit::view! {
			<Text>{format!("Its address comes from the name: “Invoices” lives at {example}")}</Text>
		});
		children.push(text("Description (optional)"));
		children.push(text("Create project"));
		children.push(repository_picker(&organization.repositories));
	} else {
		children.push(text(
			"monochange is still confirming this account with GitHub. Reload the page in a moment to create projects.",
		));
	}

	children.push(heading(2, "Repositories"));
	children.push(leptoskit::view! { <Text>{count}</Text> });
	children.push(link(organization_path(&login), "Search repositories"));
	children.push(repository_list(&page));
	leptoskit::view! { <View class="flex flex-col gap-4">{children}</View> }
}

fn repository_picker(repositories: &[crate::server_fns::repos::RepoInfo]) -> Element {
	let mut children = Vec::new();
	if repositories.len() > crate::components::repository_list::PICKER_FILTER_THRESHOLD {
		children.push(text("Filter repositories"));
	}
	for repository in repositories {
		let name = repository.github_full_name.clone();
		let label = repository.github_full_name.clone();
		children.push(leptoskit::view! { <Checkbox checked=false aria_label=name /> });
		children.push(leptoskit::view! { <Text>{label}</Text> });
	}
	leptoskit::view! { <View class="flex flex-col gap-2">{children}</View> }
}

fn repository_list(page: &crate::repository_filter::RepositoryPage) -> Element {
	if page.matches == 0 {
		return text(format!("No repositories match “{}”.", "the current search"));
	}

	let mut rows = Vec::new();
	for repository in &page.repositories {
		let visibility = if repository.github_private {
			"Private"
		} else {
			"Public"
		};
		let name = repository.github_full_name.clone();
		let mut children = vec![leptoskit::view! { <Text>{name}</Text> }, text(visibility)];
		if repository.installation_suspended {
			children.push(text("Installation suspended"));
		}
		rows.push(leptoskit::view! { <View role=Role::ListItem>{children}</View> });
	}

	if page.page_count > 1 {
		let pager = format!("Page {} of {}", page.page, page.page_count);
		rows.push(leptoskit::view! { <Text>{pager}</Text> });
	}
	leptoskit::view! { <View role=Role::List class="flex flex-col gap-2">{rows}</View> }
}

#[cfg(test)]
#[path = "__tests__/organization_tests.rs"]
mod tests;
