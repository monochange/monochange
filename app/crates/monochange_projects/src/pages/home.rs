use leptos::prelude::*;
/// The account's avatar, or its initial when GitHub hasn't sent one.
#[component]
pub fn OrganizationAvatar(
	login: String,
	#[prop(optional_no_strip)] avatar_url: Option<String>,
) -> impl IntoView {
	if let Some(url) = avatar_url {
		return view! { <img class="organization-avatar" src=url alt="" width="48" height="48" /> }
			.into_any();
	}

	let initial = login
		.chars()
		.next()
		.map_or('?', |character| character.to_ascii_uppercase());
	view! {
		<span class="organization-avatar" title=login aria-hidden="true">
			{initial.to_string()}
		</span>
	}
	.into_any()
}

/// Organization and project entry point, using the website's session authority.
#[component]
pub fn ProjectsHome() -> impl IntoView {
	let organizations = Resource::new_blocking(
		|| (),
		|()| crate::server_fns::organizations::list_organizations(),
	);
	view! {
		<section class="site-width dashboard-section">
			<h1>"Your projects"</h1>
			<p>"One place for the products you ship."</p>
			<Suspense fallback=|| view! { <p role="status">"Loading projects…"</p> }>
				{move || organizations.get().map(|result| match result {
					Ok(organizations) if organizations.is_empty() => view! { <div class="repository-empty"><h2>"Your next project starts here"</h2><p>"Sign in and connect repositories to create a project."</p><a class="button button-brand" href="/login?next=%2Fprojects" target=monochange_shared::routing::cross_surface_target("/projects", "/login")>"Sign in"</a><a class="text-link" href="/dashboard" target=monochange_shared::routing::cross_surface_target("/projects", "/dashboard")>"Connect repositories"</a></div> }.into_any(),
					Ok(organizations) => view! { <ul class="organization-list">{organizations.into_iter().map(|org| view! { <li class="organization-card"><a class="text-link" href=org.path()>{org.login.clone()}</a><p>{format!("{} projects", org.projects.len())}</p><ul>{org.projects.into_iter().map(|project| view! { <li><a href=monochange_shared::project_path(&org.login, &project.slug)>{project.name}</a><p>{project.description}</p></li> }).collect::<Vec<_>>()}</ul></li> }).collect::<Vec<_>>()}</ul> }.into_any(),
					Err(_) => view! { <p role="alert">"Projects couldn't be loaded. Please reload to try again."</p> }.into_any(),
				})}

			</Suspense>
		</section>
	}
}

/// The projects workspace has its own navigation, sharing the same browser cookie.
#[component]
pub fn ProjectsNavigation() -> impl IntoView {
	let logout = ServerAction::<crate::server_fns::auth::Logout>::new();
	// The dashboard and account screens belong to the website browser entry;
	// those links must load that entry as a document navigation.
	let to_website =
		|href: &'static str| monochange_shared::routing::cross_surface_target("/projects", href);
	view! {
		<header class="site-width"><nav class="flex items-center justify-between gap-4 py-6" aria-label="Projects navigation">
			<a class="text-xl font-bold" href="/projects">"monochange projects"</a>
			<a href="/dashboard" target=to_website("/dashboard")>"Connect repositories"</a>
			<a href="/account" target=to_website("/account")>"Account and devices"</a>
			<leptos::form::ActionForm action=logout><button type="submit">"Sign out"</button></leptos::form::ActionForm>
		</nav></header>
	}
}
