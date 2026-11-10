//! Connected repositories, with explicit loading and unavailable states.

use leptos::prelude::Get;
use leptos::prelude::Resource;
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
		crate::slots::once(|| Resource::new_blocking(|| (), |()| list_organizations()));
	let connection =
		crate::slots::once(|| Resource::new_blocking(|| (), |()| repository_connection()));
	let permissions =
		crate::slots::once(|| Resource::new_blocking(|| (), |()| permission_updates()));

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
					let filter = RepositoryFilter::new(search, organization);
					workspace(&session, &organizations, connection, permissions, &filter)
				}
			}
		}
	}
}

fn notice(title: &'static str, message: &'static str) -> Element {
	let mark = image("/branding/mark.svg");
	let title = heading(1, title);
	let message = text(message);
	let sign_in = link("/login", "Sign in with GitHub");
	let book = link(BOOK_URL, "Read the book");
	leptoskit::view! {
		<View class="flex flex-col gap-4">
			{mark}
			{title}
			{message}
			{sign_in}
			{book}
		</View>
	}
}

fn workspace(
	user: &SessionUser,
	organizations: &[OrganizationSummary],
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
	permissions: Resource<Result<Vec<PermissionUpdate>, server_fn::ServerFnError>>,
	filter: &RepositoryFilter,
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

	let mut children = vec![
		avatar(user.github_avatar_url.clone(), &user.github_login),
		leptoskit::view! { <Text>{format!("Welcome, {}", user.github_login)}</Text> },
		text("Your free monochange workspace."),
		permission_notice(permissions),
		connection_panel(connection),
		heading(2, "Organisations"),
		leptoskit::view! { <Text>{heading_count}</Text> },
	];

	if organizations.is_empty() {
		children.push(heading(3, "No repositories connected yet"));
		children.push(text(
			"The installation guide explains GitHub App availability and repository setup.",
		));
		children.push(link("/install#github-app", "Read the setup guide"));
	} else {
		children.push(filters(filter, organizations));
		for organization in organizations {
			children.push(organization_card(organization, filter));
		}
		if matches == 0 {
			children.push(heading(3, "No repositories match"));
			children.push(text("Try a shorter name, or look in every account."));
			children.push(link("/dashboard", "Clear filters"));
		}
	}

	children.push(heading(2, "Plan a release from your CLI."));
	children.push(text(
		"Use the CLI locally or in your existing CI workflow to configure and review each repository's release plan.",
	));
	children.push(link(BOOK_URL, "Open the monochange book"));
	leptoskit::view! { <View class="flex flex-col gap-4">{children}</View> }
}

fn filters(filter: &RepositoryFilter, organizations: &[OrganizationSummary]) -> Element {
	let query = format!("Search repositories {}", filter.search);
	let mut children = vec![leptoskit::view! { <Text>{query}</Text> }];
	if organizations.len() > 1 {
		children.push(link("/dashboard", "All accounts"));
		for organization in organizations {
			let href = RepositoryFilter {
				search: filter.search.clone(),
				organization: Some(organization.login.clone()),
			}
			.href("/dashboard");
			children.push(link(href, &organization.login));
		}
	}
	leptoskit::view! {
		<View
			role=Role::Search
			aria_label="Filter repositories"
			class="flex flex-row flex-wrap gap-2"
		>
			{children}
		</View>
	}
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
	let login = organization.login.clone();
	let mut children = vec![
		avatar(organization.avatar_url.clone(), &organization.login),
		leptoskit::view! { <Text>{login}</Text> },
		leptoskit::view! { <Text>{summary}</Text> },
		link(
			organization.path(),
			&format!("Open {} to manage projects", organization.login),
		),
	];

	if matches > 0 || !filter.is_active() {
		for repository in &organization.repositories {
			if crate::repository_filter::matches_search(repository, &filter.search)
				&& filter.includes_organization(&organization.login)
			{
				children.push(repository_row(repository));
			}
		}
	}

	leptoskit::view! { <View class="flex flex-col gap-2">{children}</View> }
}

fn repository_row(repository: &crate::server_fns::repos::RepoInfo) -> Element {
	let name = repository
		.github_full_name
		.rsplit_once('/')
		.map_or(repository.github_full_name.as_str(), |(_, name)| name);
	let label = repository.github_full_name.clone();
	let full_name = repository.github_full_name.clone();
	let mut children = vec![
		leptoskit::view! { <Text aria_label=label>{name}</Text> },
		leptoskit::view! { <Text>{full_name}</Text> },
	];
	if repository.installation_suspended {
		children.push(text("Installation suspended"));
	}
	let visibility = if repository.github_private {
		"Private"
	} else {
		"Public"
	};
	children.push(text(visibility));
	leptoskit::view! { <View class="flex flex-row gap-2">{children}</View> }
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
	let rendered = leptoskit::web::render(organization_avatar(&login, avatar_url));
	drop(login);
	rendered
}

fn avatar(avatar_url: Option<String>, login: &str) -> Element {
	if let Some(url) = avatar_url {
		return leptoskit::view! { <Image source=url aria_label="" /> };
	}
	let initial = login
		.chars()
		.next()
		.map_or('?', |character| character.to_ascii_uppercase());
	let initial = initial.to_string();
	let login = login.to_owned();
	leptoskit::view! { <Text aria_label=login>{initial}</Text> }
}

fn permission_notice(
	permissions: Resource<Result<Vec<PermissionUpdate>, server_fn::ServerFnError>>,
) -> Element {
	let Some(Ok(updates)) = permissions.get() else {
		return leptoskit::view! { <View /> };
	};
	if updates.is_empty() {
		return leptoskit::view! { <View /> };
	}

	let mut children = vec![
		heading(2, "Approve monochange's new GitHub permissions"),
		text(
			"monochange now asks GitHub for more access. Until an owner of each account below approves the request, monochange keeps the access it has today. You don't need to sign in again.",
		),
	];
	for update in updates {
		let account = update.account;
		let missing = update.missing;
		children.push(leptoskit::view! { <Text>{account}</Text> });
		children.push(leptoskit::view! { <Text>{missing}</Text> });
		children.push(link(update.review_url, "Review on GitHub"));
	}
	leptoskit::view! { <View class="flex flex-col gap-2">{children}</View> }
}

fn connection_panel(
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
) -> Element {
	match connection.get() {
		None => text("Loading repository setup…"),
		Some(Ok(RepositoryConnectionStatus::Available { installation_url })) => {
			let title = heading(2, "Connect your repositories");
			let copy = text(
				"Choose all repositories or selected repositories on GitHub, then return to your workspace. Organization repositories are visible to the installer while they remain an organization owner.",
			);
			let action = link(installation_url, "Connect repositories on GitHub");
			leptoskit::view! {
				<View class="flex flex-col gap-2">
					{title}
					{copy}
					{action}
				</View>
			}
		}
		Some(Ok(RepositoryConnectionStatus::Unavailable)) => {
			let title = heading(2, "Repository connection is unavailable");
			let copy = text(
				"The GitHub App isn't configured on this deployment yet. You can use the CLI while repository connection is being set up.",
			);
			leptoskit::view! {
				<View class="flex flex-col gap-2">
					{title}
					{copy}
				</View>
			}
		}
		Some(Ok(RepositoryConnectionStatus::SignedOut)) => {
			link("/login", "Sign in to connect repositories")
		}
		Some(Err(_)) => {
			let title = heading(2, "Repository setup couldn't be loaded");
			let copy = text("Please reload the page to try again.");
			let action = retry(connection);
			leptoskit::view! {
				<View class="flex flex-col gap-2">
					{title}
					{copy}
					{action}
				</View>
			}
		}
	}
}

fn retry(
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
) -> Element {
	leptoskit::view! {
		<Button class="rounded-lg px-4 py-2 font-semibold" on:press=move || connection.refetch()>
			"Try again"
		</Button>
	}
}

#[cfg(test)]
#[path = "__tests__/dashboard_tests.rs"]
mod tests;
