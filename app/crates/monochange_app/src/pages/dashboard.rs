//! Connected repositories, with explicit loading and unavailable states.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::LazyRoute;
use leptos_router::lazy_route;

use crate::components::arrow::ArrowIcon;
use crate::components::repository_list::ChevronIcon;
use crate::components::repository_list::RepositoryFilters;
use crate::components::repository_list::RepositoryPager;
use crate::components::repository_list::use_repository_filter;
use crate::links::BOOK_URL;
use crate::repository_filter::RepositoryFilter;
use crate::repository_filter::count_matches;
use crate::repository_filter::repository_count as repository_count_label;
use crate::server_fns::auth::SessionUser;
use crate::server_fns::auth::get_session;
use crate::server_fns::installation::PendingPermissionView;
use crate::server_fns::installation::PermissionUpdate;
use crate::server_fns::installation::RepositoryConnectionStatus;
use crate::server_fns::installation::permission_updates;
use crate::server_fns::installation::repository_connection;
use crate::server_fns::organizations::OrganizationSummary;
use crate::server_fns::organizations::list_organizations;

/// Dashboard data, loaded before its lazily split view chunk hydrates.
pub struct DashboardRoute {
	user: Resource<Result<Option<SessionUser>, server_fn::ServerFnError>>,
	organizations: Resource<Result<Vec<OrganizationSummary>, server_fn::ServerFnError>>,
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
}

/// Dashboard page shown after login, split into its own WASM chunk.
#[lazy_route]
impl LazyRoute for DashboardRoute {
	fn data() -> Self {
		Self {
			user: Resource::new_blocking(|| (), |()| get_session()),
			organizations: Resource::new_blocking(|| (), |()| list_organizations()),
			connection: Resource::new_blocking(|| (), |()| repository_connection()),
		}
	}

	fn view(this: Self) -> AnyView {
		let DashboardRoute {
			user,
			organizations,
			connection,
		} = this;
		// GitHub lookups per installation stream in after the page.
		let permissions = Resource::new(|| (), |()| permission_updates());
		view! {
			<Title text="Your workspace — monochange" />
			<section class="site-width dashboard-section">
				<Suspense fallback=|| view! { <p role="status">"Loading your workspace…"</p> }>
					{move || user.get().map(|result| match result {
						Ok(Some(session)) => organizations.get().map(|result| match result {
							Ok(organizations) => view! { <DashboardContent user=session organizations=organizations connection=connection permissions=permissions /> }.into_any(),
							Err(_) => view! { <DashboardNotice title="Repositories couldn't be loaded" message="Please reload the page to try again." /> }.into_any(),
						}).into_any(),

						Ok(None) => view! { <DashboardNotice title="Your workspace starts here." message="Sign in with GitHub to see your connected repositories." /> }.into_any(),
						Err(_) => view! { <DashboardNotice title="Your session couldn't be loaded" message="Please sign in again to continue." /> }.into_any(),
					})}

				</Suspense>
			</section>
		}
		.into_any()
	}
}

#[component]
fn DashboardNotice(title: &'static str, message: &'static str) -> impl IntoView {
	view! {
		<div class="status-page">
			<div><img src="/branding/mark.svg" width="64" height="64" alt="" class="mx-auto" /><h1>{title}</h1><p>{message}</p><div class="actions"><a href="/login" class="button button-brand">"Sign in with GitHub"</a><a href=BOOK_URL class="text-link">"Read the book"</a></div></div>
		</div>
	}
}

#[component]
fn DashboardContent(
	user: crate::server_fns::auth::SessionUser,
	organizations: Vec<OrganizationSummary>,
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
	permissions: Resource<Result<Vec<PermissionUpdate>, server_fn::ServerFnError>>,
) -> impl IntoView {
	let filter = use_repository_filter();
	let repository_count: usize = organizations
		.iter()
		.map(|organization| organization.repositories.len())
		.sum();
	// Only accounts and repository names are needed to count matches.
	let searchable = StoredValue::new(
		organizations
			.iter()
			.map(|organization| {
				(
					organization.login.clone(),
					organization.repositories.clone(),
				)
			})
			.collect::<Vec<_>>(),
	);
	let matches = Memo::new(move |_| {
		let filter = filter.get();
		searchable.with_value(|accounts| {
			accounts
				.iter()
				.filter(|(login, _)| filter.includes_organization(login))
				.map(|(_, repositories)| count_matches(repositories, &filter.search))
				.sum::<usize>()
		})
	});
	let logins: Vec<String> = organizations
		.iter()
		.map(|organization| organization.login.clone())
		.collect();
	let open_by_default = organizations.len() == 1;

	view! {
		<div>
			<div class="workspace-heading">
				{user.github_avatar_url.map(|url| view! { <img src=url alt="" width="56" height="56" /> })}
				<div><h1>"Welcome, " {user.github_login}</h1><p>"Your free monochange workspace."</p></div>
			</div>
			<PermissionUpdateNotice permissions=permissions />
			<RepositoryConnectionPanel connection=connection />
			<section class="repository-section" id="organizations" aria-labelledby="organizations-title">
				<div class="repository-heading"><h2 id="organizations-title">"Organisations"</h2><p aria-live="polite">{move || if filter.get().is_active() { format!("{} of {} match", matches.get(), repository_count_label(repository_count)) } else { format!("{} connected", repository_count_label(repository_count)) }}</p></div>

				{if organizations.is_empty() {
					view! {
						<div class="repository-empty"><h3>"No repositories connected yet"</h3><p>"The installation guide explains GitHub App availability and repository setup."</p><a href="/install#github-app" class="button button-brand">"Read the setup guide"</a></div>
					}.into_any()
				} else {
					view! {
						<RepositoryFilters filter=filter path="/dashboard".to_owned() organizations=logins />
						<ul class="organization-list">
							{organizations.into_iter().map(|organization| view! { <OrganizationCard organization=organization filter=filter open_by_default=open_by_default /> }).collect::<Vec<_>>()}
						</ul>
						<div class="repository-empty" hidden=move || matches.get() != 0><h3>"No repositories match"</h3><p>"Try a shorter name, or look in every account."</p><a href="/dashboard" class="button button-quiet">"Clear filters"</a></div>
					}.into_any()
				}}

			</section>
			<div class="workspace-guide"><h2>"Plan a release from your CLI."</h2><p>"Use the CLI locally or in your existing CI workflow to configure and review each repository's release plan."</p><a href=BOOK_URL class="text-link">"Open the monochange book" <ArrowIcon /></a></div>
		</div>
	}
}

/// An organisation that folds open to its repositories, which follow the
/// dashboard's filters.
#[component]
fn OrganizationCard(
	organization: OrganizationSummary,
	filter: Memo<RepositoryFilter>,
	/// Open without a filter, for a workspace with a single account.
	open_by_default: bool,
) -> impl IntoView {
	let path = organization.path();
	let login = organization.login.clone();
	let total = organization.repositories.len();
	let kind = if organization.personal {
		"Personal account"
	} else {
		"Organisation"
	};

	let projects = match organization.projects.len() {
		1 => "1 project".to_string(),
		count => format!("{count} projects"),
	};
	let searchable = StoredValue::new(organization.repositories.clone());
	let account = login.clone();
	let matches = Memo::new(move |_| {
		let filter = filter.get();
		if filter.includes_organization(&account) {
			searchable.with_value(|repositories| count_matches(repositories, &filter.search))
		} else {
			0
		}
	});
	let summary = move || {
		if filter.get().is_active() {
			format!(
				"{kind} · {} of {} match",
				matches.get(),
				repository_count_label(total)
			)
		} else {
			format!("{kind} · {} · {projects}", repository_count_label(total))
		}
	};
	let search = Signal::derive(move || filter.get().search);
	// A filter opens the card to show its matches. Clearing the filter leaves
	// it open, so the list doesn't fold away under the reader.
	let open = Memo::new(move |was_open: Option<&bool>| {
		open_by_default || filter.get().is_active() || was_open.copied().unwrap_or(false)
	});

	view! {
		<li class="organization-card" hidden=move || matches.get() == 0>
			<details open=move || open.get()>
				<summary class="organization-summary">
					{organization_avatar(&login, organization.avatar_url.clone())}
					<div><h3>{login.clone()}</h3><p>{summary}</p></div>
					<ChevronIcon />
				</summary>
				<div class="organization-body">
					<RepositoryPager repositories=organization.repositories search=search label=format!("{login} repositories") />
					<a href=path class="text-link">"Open " {login.clone()} " to manage projects" <ArrowIcon /></a>
				</div>
			</details>
		</li>
	}
}

/// The account's avatar, or its initial when GitHub hasn't sent one.
pub fn organization_avatar(login: &str, avatar_url: Option<String>) -> AnyView {
	if let Some(url) = avatar_url {
		return view! { <img class="organization-avatar" src=url alt="" width="48" height="48" /> }
			.into_any();
	}

	let initial = login
		.chars()
		.next()
		.map_or('?', |character| character.to_ascii_uppercase());
	view! { <span class="organization-avatar" aria-hidden="true">{initial.to_string()}</span> }
		.into_any()
}

/// Accounts whose installation is waiting for an owner to approve the app's
/// new permissions. Hidden while loading, on error, and when none are waiting.
#[component]
fn PermissionUpdateNotice(
	permissions: Resource<Result<Vec<PermissionUpdate>, server_fn::ServerFnError>>,
) -> impl IntoView {
	view! {
		<Suspense fallback=|| ()>
			{move || permissions.get().and_then(Result::ok).filter(|updates| !updates.is_empty()).map(|updates| view! {
				<section class="permission-update" aria-labelledby="permission-update-title">
					<h2 id="permission-update-title">"Approve monochange's new GitHub permissions"</h2>
					<p>"monochange now asks GitHub for more access. Until an owner of each account below approves the request, monochange keeps the access it has today. You don't need to sign in again."</p>
					<ul>
						{updates.into_iter().map(|update| view! {
							<li>
								<div><h3>{update.account}</h3><p>{update.permissions.iter().map(PendingPermissionView::label).collect::<Vec<_>>().join(", ")}</p></div>
								<a href=update.review_url rel="external" class="button button-quiet">"Review on GitHub"</a>
							</li>
						}).collect::<Vec<_>>()}
					</ul>
				</section>
			})}
		</Suspense>
	}
}

#[component]
fn RepositoryConnectionPanel(
	connection: Resource<Result<RepositoryConnectionStatus, server_fn::ServerFnError>>,
) -> impl IntoView {
	view! {
		<section class="repository-empty" aria-label="Connect repositories">
			<Suspense fallback=|| view! { <p role="status">"Loading repository setup…"</p> }>
				{move || connection.get().map(|result| match result {
					Ok(RepositoryConnectionStatus::Available { installation_url }) => view! {
						<h2>"Connect your repositories"</h2>
						<p>"Choose all repositories or selected repositories on GitHub, then return to your workspace. Organization repositories are visible to the installer while they remain an organization owner."</p>
						<a href=installation_url rel="external" class="button button-brand">"Connect repositories on GitHub"</a>
					}.into_any(),
					Ok(RepositoryConnectionStatus::Unavailable) => view! {
						<h2>"Repository connection is unavailable"</h2>
						<p>"The GitHub App isn't configured on this deployment yet. You can use the CLI while repository connection is being set up."</p>
					}.into_any(),
					Ok(RepositoryConnectionStatus::SignedOut) => view! { <a href="/login" class="button button-brand">"Sign in to connect repositories"</a> }.into_any(),
					Err(_) => view! {
						<h2>"Repository setup couldn't be loaded"</h2>
						<p>"Please reload the page to try again."</p>
						<button type="button" class="text-link" on:click=move |_| connection.refetch()>"Try again"</button>
					}.into_any(),
				})}

			</Suspense>
		</section>
	}
}

#[cfg(test)]
#[path = "__tests__/dashboard_tests.rs"]
mod tests;
