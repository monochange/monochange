use leptos::prelude::*;
use monochange_shared::OrganizationSummary;
use monochange_shared::ProjectSummary;

use crate::components::arrow::ArrowIcon;

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

/// One project as a card linking to its page at `href`: the workspace landing
/// and the organisation page show projects the same way.
/// Descendants are snapshot text and native navigation; WASM handlers and
/// reactive children must be rendered outside the serialized content.
#[component]
pub fn ProjectCard(href: String, project: ProjectSummary) -> AnyView {
	let count = match project.repository_count {
		1 => "1 repository".to_owned(),
		count => format!("{count} repositories"),
	};

	// This snapshot has no WASM handlers or reactive children. Framework HTML
	// rendering preserves escaping without retaining its async hydration tree.
	let content = view! {
		<a href=href class="project-card">
			<h3>{project.name}</h3>
			{(!project.description.is_empty()).then(|| view! { <p>{project.description}</p> })}
			<span>{count} <ArrowIcon /></span>
		</a>
	}
	.to_html();

	view! { <li inner_html=content /> }.into_any()
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
			<h1>"Your projects"</h1>
			<p class="projects-intro">"One place for the products you ship."</p>
			<Suspense fallback=|| view! { <p class="project-empty" role="status">"Loading projects…"</p> }>
				{move || workspace.map(|result| match result {
					Ok((None, _)) => view! { <WorkspaceEmpty signed_in=false /> }.into_any(),
					Ok((Some(_), organizations)) if organizations.is_empty() => view! { <WorkspaceEmpty signed_in=true /> }.into_any(),
					Ok((Some(_), organizations)) => view! { <ul>{organizations.iter().cloned().map(|organization| view! { <WorkspaceOrganization organization=organization /> }.into_inner()).collect::<Vec<_>>()}</ul> }.into_any(),
					Err(_) => view! { <p class="project-empty" role="alert">"Projects couldn't be loaded. Please reload to try again."</p> }.into_any(),
				})}

			</Suspense>
		</section>
	}
}

/// One connected account on the workspace landing: its heading, then its
/// projects as cards.
#[component]
fn WorkspaceOrganization(organization: OrganizationSummary) -> AnyView {
	let path = organization.path();
	let login = organization.login.clone();
	let count = match organization.projects.len() {
		1 => "1 project".to_owned(),
		count => format!("{count} projects"),
	};

	view! {
		<li class="workspace-organization">
			<div class="workspace-organization-heading">
				<OrganizationAvatar login=login.clone() avatar_url=organization.avatar_url />
				<div><h2><a href=path.clone()>{login.clone()}</a></h2><p>{count}</p></div>
			</div>
			{if organization.projects.is_empty() {
				view! { <p class="project-empty">"No projects yet. " <a href=path class="text-link">"Open " {login} " to manage projects" <ArrowIcon /></a></p> }.into_any()
			} else {
				view! { <ul class="project-grid">{organization.projects.into_iter().map(|project| view! { <ProjectCard href=monochange_shared::project_path(&login, &project.slug) project=project /> }.into_inner()).collect::<Vec<_>>()}</ul> }.into_any()
			}}
		</li>
	}
	.into_any()
}

#[component]
fn WorkspaceEmpty(signed_in: bool) -> impl IntoView {
	let (href, label) = if signed_in {
		("/dashboard", "Connect repositories")
	} else {
		("/login?next=%2Fprojects", "Sign in with GitHub")
	};
	view! {
		<div class="projects-empty">
			<div class="projects-empty-mark"><img src="/branding/mark.svg" width="48" height="48" alt="" /></div>
			<div>
				<h2>"Your next project starts here"</h2>
				<p>"Bring your repositories together in a workspace for the products you ship."</p>
				<div class="actions">
					<a class="button button-brand" href=href target="_self">{label}</a>
					<a class="text-link" href="/book" target="_self">"Read the book"</a>
				</div>
			</div>
		</div>
	}
}
