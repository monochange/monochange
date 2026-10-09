//! Connected repositories, with explicit loading and unavailable states.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::links::BOOK_URL;
use crate::server_fns::auth::get_session;
use crate::server_fns::installation::RepositoryConnectionStatus;
use crate::server_fns::installation::repository_connection;
use crate::server_fns::organizations::OrganizationSummary;
use crate::server_fns::organizations::list_organizations;

/// Dashboard page shown after login.
#[component]
pub fn DashboardPage() -> impl IntoView {
	let user = Resource::new_blocking(|| (), |()| get_session());
	let organizations = Resource::new_blocking(|| (), |()| list_organizations());
	let connection = Resource::new_blocking(|| (), |()| repository_connection());

	view! {
		<Title text="Your workspace — monochange" />
		<section class="site-width dashboard-section">
			<Suspense fallback=|| view! { <p role="status">"Loading your workspace…"</p> }>
				{move || user.get().map(|result| match result {
					Ok(Some(session)) => organizations.get().map(|result| match result {
						Ok(organizations) => view! { <DashboardContent user=session organizations=organizations connection=connection /> }.into_any(),
						Err(_) => view! { <DashboardNotice title="Repositories couldn't be loaded" message="Please reload the page to try again." /> }.into_any(),
					}).into_any(),
					Ok(None) => view! { <DashboardNotice title="Your workspace starts here." message="Sign in with GitHub to see your connected repositories." /> }.into_any(),
					Err(_) => view! { <DashboardNotice title="Your session couldn't be loaded" message="Please sign in again to continue." /> }.into_any(),
				})}
			</Suspense>
		</section>
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
) -> impl IntoView {
	let repository_count: usize = organizations
		.iter()
		.map(|organization| organization.repositories.len())
		.sum();
	view! {
		<div>
			<div class="workspace-heading">
				{user.github_avatar_url.map(|url| view! { <img src=url alt="" width="56" height="56" /> })}
				<div><h1>"Welcome, " {user.github_login}</h1><p>"Your free monochange workspace."</p></div>
			</div>
			<RepositoryConnectionPanel connection=connection />
			<section class="repository-section" aria-labelledby="organizations-title">
				<div class="repository-heading"><h2 id="organizations-title">"Organisations"</h2><p>{repository_count} " repositories connected"</p></div>
				{if organizations.is_empty() {
					view! {
						<div class="repository-empty"><h3>"No repositories connected yet"</h3><p>"The installation guide explains GitHub App availability and repository setup."</p><a href="/install#github-app" class="button button-brand">"Read the setup guide"</a></div>
					}.into_any()
				} else {
					view! {
						<ul class="organization-list">
							{organizations.into_iter().map(|organization| view! { <OrganizationCard organization=organization /> }).collect::<Vec<_>>()}
						</ul>
					}.into_any()
				}}
			</section>
			<div class="workspace-guide"><h2>"Plan a release from your CLI."</h2><p>"Use the CLI locally or in your existing CI workflow to configure and review each repository's release plan."</p><a href=BOOK_URL class="text-link">"Open the monochange book" <ArrowIcon /></a></div>
		</div>
	}
}

/// An organisation with its repositories, linking to its projects.
#[component]
fn OrganizationCard(organization: OrganizationSummary) -> impl IntoView {
	let path = organization.path();
	let kind = if organization.personal {
		"Personal account"
	} else {
		"Organisation"
	};
	let projects = match organization.projects.len() {
		1 => "1 project".to_string(),
		count => format!("{count} projects"),
	};
	view! {
		<li class="organization-card">
			<a href=path class="organization-link">
				{organization_avatar(&organization.login, organization.avatar_url.clone())}
				<div><h3>{organization.login.clone()}</h3><p>{kind} " · " {projects}</p></div>
				<ArrowIcon />
			</a>
			<ul class="repository-list">
				{organization.repositories.into_iter().map(|repo| view! {
					<li><div><h3>{repo.github_full_name}</h3><p>{if repo.github_private { "Private" } else { "Public" }} " · " {if repo.installation_suspended { "Installation suspended".to_string() } else { format!("Connected via {}", repo.installation_login) }}</p></div><span>"Free"</span></li>
				}).collect::<Vec<_>>()}
			</ul>
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
