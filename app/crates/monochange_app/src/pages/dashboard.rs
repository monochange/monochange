//! Connected repositories, with explicit loading and unavailable states.

use leptos::prelude::*;
use leptoskit::native::Role;
use leptoskit::prelude::*;

use crate::links::BOOK_URL;
use crate::repository_filter::RepositoryFilter;
use crate::repository_filter::count_matches;
use crate::repository_filter::repository_count;
use crate::server_fns::auth::SessionUser;
use crate::server_fns::installation::PermissionUpdate;
use crate::server_fns::installation::RepositoryConnectionStatus;
use crate::server_fns::installation::permission_updates;
use crate::server_fns::installation::repository_connection;
use crate::server_fns::organizations::OrganizationSummary;
use crate::server_fns::organizations::list_organizations;
use crate::ui::heading;
use crate::ui::image;
use crate::ui::link;
use crate::ui::text;

/// Workspace screen for `/dashboard`.
pub fn dashboard_screen(search: Option<String>, organization: Option<String>) -> Element {
	let user = crate::slots::session();
	let organizations =
		crate::slots::once(|| Resource::new_blocking(|| (), |_| list_organizations()));
	let connection =
		crate::slots::once(|| Resource::new_blocking(|| (), |_| repository_connection()));
	let permissions =
		crate::slots::once(|| Resource::new_blocking(|| (), |_| permission_updates()));

	match user.get() {
		None => text("Loading your workspace…"),
		Some(Ok(None)) => {
			notice(
				"Your workspace starts here.",
				"Sign in with GitHub to see your connected repositories.",
			)
		}
		Some(Err(_)) => {
			notice(
				"Your session couldn't be loaded",
				"Please sign in again to continue.",
			)
		}
		Some(Ok(Some(session))) => {
			match organizations.get() {
				None => text("Loading your workspace…"),
				Some(Err(_)) => {
					notice(
						"Repositories couldn't be loaded",
						"Please reload the page to try again.",
					)
				}
				Some(Ok(organizations)) => {
					workspace(
						session,
						organizations,
						connection,
						permissions,
						RepositoryFilter::new(search, organization),
					)
				}
			}
		}
	}
}

fn notice(title: &'static str, message: &'static str) -> Element {
	View()
		.class("flex flex-col gap-4")
		.child(image("/branding/mark.svg"))
		.child(heading(1, title))
		.child(text(message))
		.child(link("/login", "Sign in with GitHub"))
		.child(link(BOOK_URL, "Read the book"))
}

fn workspace(
	user: SessionUser,
	organizations: Vec<OrganizationSummary>,
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
	permissions: Resource<Result<Vec<PermissionUpdate>, server_fn::ServerFnError>>,
	filter: RepositoryFilter,
) -> Element {
	let repository_total: usize = organizations
		.iter()
		.map(|organization| organization.repositories.len())
		.sum();
	let matches: usize = organizations
		.iter()
		.filter(|organization| filter.includes_organization(&organization.login))
		.map(|organization| count_matches(&organization.repositories, &filter.search))
		.sum();
	let heading_count = if filter.is_active() {
		format!("{matches} of {} match", repository_count(repository_total))
	} else {
		format!("{} connected", repository_count(repository_total))
	};

	let mut page = View()
		.class("flex flex-col gap-4")
		.child(avatar(user.github_avatar_url.clone(), &user.github_login))
		.child(Text(format!("Welcome, {}", user.github_login)))
		.child(text("Your free monochange workspace."))
		.child(permission_notice(permissions))
		.child(connection_panel(connection))
		.child(heading(2, "Organisations"))
		.child(Text(heading_count));

	if organizations.is_empty() {
		page = page
			.child(heading(3, "No repositories connected yet"))
			.child(text(
				"The installation guide explains GitHub App availability and repository setup.",
			))
			.child(link("/install#github-app", "Read the setup guide"));
	} else {
		page = page.child(filters(&filter, &organizations));
		for organization in &organizations {
			page = page.child(organization_card(organization, &filter));
		}
		if matches == 0 {
			page = page
				.child(heading(3, "No repositories match"))
				.child(text("Try a shorter name, or look in every account."))
				.child(link("/dashboard", "Clear filters"));
		}
	}

	page.child(heading(2, "Plan a release from your CLI."))
		.child(text(
			"Use the CLI locally or in your existing CI workflow to configure and review each repository's release plan.",
		))
		.child(link(BOOK_URL, "Open the monochange book"))
}

fn filters(filter: &RepositoryFilter, organizations: &[OrganizationSummary]) -> Element {
	let mut row = View()
		.role(Role::Search)
		.aria_label("Filter repositories")
		.class("flex flex-row flex-wrap gap-2")
		.child(Text(format!("Search repositories {}", filter.search)));

	if organizations.len() > 1 {
		row = row.child(link("/dashboard", "All accounts"));
		for organization in organizations {
			let href = RepositoryFilter {
				search: filter.search.clone(),
				organization: Some(organization.login.clone()),
			}
			.href("/dashboard");
			row = row.child(link(href, &organization.login));
		}
	}

	row
}

fn organization_card(organization: &OrganizationSummary, filter: &RepositoryFilter) -> Element {
	let total = organization.repositories.len();
	let matches = if filter.includes_organization(&organization.login) {
		count_matches(&organization.repositories, &filter.search)
	} else {
		0
	};
	let kind = if organization.personal {
		"Personal account"
	} else {
		"Organisation"
	};
	let projects = match organization.projects.len() {
		1 => "1 project".to_owned(),
		count => format!("{count} projects"),
	};
	let summary = if filter.is_active() {
		format!("{kind} · {matches} of {} match", repository_count(total))
	} else {
		format!("{kind} · {} · {projects}", repository_count(total))
	};
	let mut card = View()
		.class("flex flex-col gap-2")
		.child(avatar(organization.avatar_url.clone(), &organization.login))
		.child(Text(organization.login.clone()))
		.child(Text(summary))
		.child(link(
			organization.path(),
			&format!("Open {} to manage projects", organization.login),
		));

	if matches > 0 || !filter.is_active() {
		for repository in &organization.repositories {
			if crate::repository_filter::matches_search(repository, &filter.search)
				&& filter.includes_organization(&organization.login)
			{
				card = card.child(repository_row(repository));
			}
		}
	}

	card
}

fn repository_row(repository: &crate::server_fns::repos::RepoInfo) -> Element {
	let name = repository
		.github_full_name
		.rsplit_once('/')
		.map_or(repository.github_full_name.as_str(), |(_, name)| name);
	let mut row = View()
		.class("flex flex-row gap-2")
		.child(Text(name).aria_label(repository.github_full_name.clone()))
		.child(Text(repository.github_full_name.clone()));
	if repository.installation_suspended {
		row = row.child(text("Installation suspended"));
	}
	let visibility = if repository.github_private {
		"Private"
	} else {
		"Public"
	};
	row.child(text(visibility))
}

/// The account's avatar, or its initial when GitHub hasn't sent one.
pub fn organization_avatar(login: &str, avatar_url: Option<String>) -> Element {
	avatar(avatar_url, login)
}

/// Leptos wrapper so existing organisation markup can embed the avatar.
#[leptos::prelude::component]
pub fn OrganizationAvatar(
	login: String,
	#[prop(optional_no_strip)] avatar_url: Option<String>,
) -> impl leptos::prelude::IntoView {
	leptoskit::web::render(organization_avatar(&login, avatar_url))
}

fn avatar(avatar_url: Option<String>, login: &str) -> Element {
	if let Some(url) = avatar_url {
		return Image(url).aria_label("");
	}
	let initial = login
		.chars()
		.next()
		.map_or('?', |character| character.to_ascii_uppercase());
	Text(initial.to_string()).aria_label(login)
}

fn permission_notice(
	permissions: Resource<Result<Vec<PermissionUpdate>, server_fn::ServerFnError>>,
) -> Element {
	let Some(Ok(updates)) = permissions.get() else {
		return View();
	};
	if updates.is_empty() {
		return View();
	}

	let mut section = View()
		.class("flex flex-col gap-2")
		.child(heading(2, "Approve monochange's new GitHub permissions"))
		.child(text(
			"monochange now asks GitHub for more access. Until an owner of each account below approves the request, monochange keeps the access it has today. You don't need to sign in again.",
		));
	for update in updates {
		section = section
			.child(Text(update.account))
			.child(Text(update.missing))
			.child(link(update.review_url, "Review on GitHub"));
	}
	section
}

fn connection_panel(
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
) -> Element {
	match connection.get() {
		None => text("Loading repository setup…"),
		Some(Ok(RepositoryConnectionStatus::Available { installation_url })) => View()
			.class("flex flex-col gap-2")
			.child(heading(2, "Connect your repositories"))
			.child(text(
				"Choose all repositories or selected repositories on GitHub, then return to your workspace. Organization repositories are visible to the installer while they remain an organization owner.",
			))
			.child(link(installation_url, "Connect repositories on GitHub")),
		Some(Ok(RepositoryConnectionStatus::Unavailable)) => View()
			.class("flex flex-col gap-2")
			.child(heading(2, "Repository connection is unavailable"))
			.child(text(
				"The GitHub App isn't configured on this deployment yet. You can use the CLI while repository connection is being set up.",
			)),
		Some(Ok(RepositoryConnectionStatus::SignedOut)) => link("/login", "Sign in to connect repositories"),
		Some(Err(_)) => View()
			.class("flex flex-col gap-2")
			.child(heading(2, "Repository setup couldn't be loaded"))
			.child(text("Please reload the page to try again."))
			.child(retry(connection)),
	}
}

fn retry(
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
) -> Element {
	Button("Try again")
		.class("rounded-lg px-4 py-2 font-semibold")
		.on_press(move || connection.refetch())
}

#[cfg(test)]
#[path = "__tests__/dashboard_tests.rs"]
mod tests;
