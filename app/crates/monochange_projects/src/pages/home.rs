use leptos::prelude::*;

#[cfg(test)]
#[path = "__tests__/home_tests.rs"]
mod tests;

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
	let workspace = Resource::new_blocking(
		|| (),
		|()| {
			async {
				let user = crate::server_fns::auth::get_session().await?;
				let organizations = if user.is_some() {
					crate::server_fns::organizations::list_organizations().await?
				} else {
					Vec::new()
				};
				Ok::<_, server_fn::ServerFnError>((user, organizations))
			}
		},
	);
	view! {
		<section class="site-width dashboard-section projects-home">
			<p class="projects-eyebrow">"YOUR WORKSPACE"</p>
			<h1>"Your projects"</h1>
			<p class="projects-intro">"One place for the products you ship."</p>
			<Suspense fallback=|| view! { <p role="status">"Loading projects…"</p> }>
				{move || workspace.get().map(|result| match result {
					Ok((None, _)) => view! { <WorkspaceEmpty signed_in=false /> }.into_any(),
					Ok((Some(_), organizations)) if organizations.is_empty() => view! { <WorkspaceEmpty signed_in=true /> }.into_any(),
					Ok((Some(_), organizations)) => view! { <ul class="organization-list">{organizations.into_iter().map(|org| view! { <li class="organization-card"><a class="text-link" href=org.path()>{org.login.clone()}</a><p>{format!("{} projects", org.projects.len())}</p><ul>{org.projects.into_iter().map(|project| view! { <li><a href=monochange_shared::project_path(&org.login, &project.slug)>{project.name}</a><p>{project.description}</p></li> }).collect::<Vec<_>>()}</ul></li> }).collect::<Vec<_>>()}</ul> }.into_any(),
					Err(_) => view! { <p role="alert">"Projects couldn't be loaded. Please reload to try again."</p> }.into_any(),
				})}

			</Suspense>
		</section>
	}
}

#[component]
fn WorkspaceEmpty(signed_in: bool) -> impl IntoView {
	view! {
		<div class="projects-empty">
			<div class="projects-empty-mark"><img src="/branding/mark.svg" width="48" height="48" alt="" /></div>
			<div>
				<p class="projects-eyebrow">"START HERE"</p>
				<h2>"Your next project starts here"</h2>
				<p>"Bring your repositories together in a workspace for the products you ship."</p>
				<div class="actions">
					{if signed_in {
						view! { <a class="button button-brand" href="/dashboard" target="_self">"Connect repositories"</a> }.into_any()
					} else {
						view! { <a class="button button-brand" href="/login?next=%2Fprojects" target="_self">"Sign in with GitHub"</a> }.into_any()
					}}
					<a class="text-link" href="/book" target="_self">"Read the book"</a>
				</div>
			</div>
		</div>
	}
}
