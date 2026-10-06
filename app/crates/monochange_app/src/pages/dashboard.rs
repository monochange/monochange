//! Connected repositories, with explicit loading and unavailable states.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::links::BOOK_URL;
use crate::server_fns::auth::get_session;
use crate::server_fns::repos::list_repos;

/// Dashboard page shown after login.
#[component]
pub fn DashboardPage() -> impl IntoView {
	let user = Resource::new(|| (), |()| get_session());
	let repos = Resource::new(|| (), |()| list_repos());

	view! {
		<Title text="Your workspace — monochange" />
		<section class="site-width dashboard-section">
			<Suspense fallback=|| view! { <p role="status">"Loading your workspace…"</p> }>
				{move || user.get().map(|result| match result {
					Ok(Some(session)) => repos.get().map(|result| match result {
						Ok(repositories) => view! { <DashboardContent user=session repos=repositories /> }.into_any(),
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
	repos: Vec<crate::server_fns::repos::RepoInfo>,
) -> impl IntoView {
	view! {
		<div>
			<div class="workspace-heading">
				{user.github_avatar_url.map(|url| view! { <img src=url alt="" width="56" height="56" /> })}
				<div><h1>"Welcome, " {user.github_login}</h1><p>"Your free monochange workspace."</p></div>
			</div>
			<section class="repository-section" aria-labelledby="repositories-title">
				<div class="repository-heading"><h2 id="repositories-title">"Repositories"</h2><p>{repos.len()} " connected"</p></div>
				{if repos.is_empty() {
					view! {
						<div class="repository-empty"><h3>"No repositories connected yet"</h3><p>"The installation guide explains GitHub App availability and repository setup."</p><a href="/install#github-app" class="button button-brand">"Read the setup guide"</a></div>
					}.into_any()
				} else {
					view! {
						<ul class="repository-list">
							{repos.into_iter().map(|repo| view! {
								<li><div><h3>{repo.github_full_name}</h3><p>{if repo.github_private { "Private" } else { "Public" }} " · " {if repo.installation_suspended { "Installation suspended".to_string() } else { format!("Connected via {}", repo.installation_login) }}</p></div><span>"Free"</span></li>
							}).collect::<Vec<_>>()}
						</ul>
					}.into_any()
				}}
			</section>
			<div class="workspace-guide"><h2>"Plan a release from your CLI."</h2><p>"Hosted bot installation is being finalized. You can already use the CLI locally or in your existing CI workflow."</p><a href=BOOK_URL class="text-link">"Open the monochange book" <ArrowIcon /></a></div>
		</div>
	}
}
