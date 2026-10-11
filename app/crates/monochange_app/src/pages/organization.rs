//! One organisation: its projects, a form to create one, and its connected
//! repositories.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

use crate::components::arrow::ArrowIcon;
use crate::pages::dashboard::organization_avatar;
use crate::projects::MAX_DESCRIPTION_CHARS;
use crate::projects::MAX_NAME_CHARS;
use crate::server_fns::organizations::CreateProject;
use crate::server_fns::organizations::OrganizationSummary;
use crate::server_fns::organizations::organization_overview;
use crate::server_fns::organizations::project_path;

/// The message to show for a failed server action.
pub fn action_error(error: &server_fn::ServerFnError) -> String {
	match error {
		server_fn::ServerFnError::ServerError(message) => message.clone(),
		other => other.to_string(),
	}
}

#[component]
pub fn OrganizationPage() -> impl IntoView {
	let params = use_params_map();
	let login = move || params.read().get("organization").unwrap_or_default();
	let overview = Resource::new_blocking(login, organization_overview);

	view! {
		<Title text=move || format!("{} — monochange", login()) />
		<section class="site-width dashboard-section">
			<Suspense fallback=|| view! { <p role="status">"Loading organisation…"</p> }>
				{move || overview.get().map(|result| match result {
					Ok(Some(organization)) => view! { <OrganizationContent organization=organization /> }.into_any(),
					Ok(None) => view! { <Unavailable title="Organisation not found" message="It isn't connected, or you're not one of its owners." /> }.into_any(),
					Err(_) => view! { <Unavailable title="This organisation couldn't be loaded" message="Please reload the page to try again." /> }.into_any(),
				})}
			</Suspense>
		</section>
	}
}

/// A not-found or failure state with a way back.
#[component]
pub fn Unavailable(title: &'static str, message: &'static str) -> impl IntoView {
	view! {
		<div class="status-page">
			<div><h1>{title}</h1><p>{message}</p><div class="actions"><a href="/dashboard" class="button button-brand">"Back to your workspace"</a></div></div>
		</div>
	}
}

#[component]
fn OrganizationContent(organization: OrganizationSummary) -> impl IntoView {
	let kind = if organization.personal {
		"Personal account"
	} else {
		"GitHub organisation"
	};
	let login = organization.login.clone();
	view! {
		<nav class="breadcrumbs" aria-label="Breadcrumb"><a href="/dashboard">"Workspace"</a><span aria-hidden="true">"/"</span><span aria-current="page">{login.clone()}</span></nav>
		<div class="workspace-heading">
			{organization_avatar(&login, organization.avatar_url.clone())}
			<div><h1>{login.clone()}</h1><p>{kind} " · " {organization.repositories.len()} " connected repositories"</p></div>
		</div>
		<section class="repository-section" aria-labelledby="projects-title">
			<div class="repository-heading"><h2 id="projects-title">"Projects"</h2><p>"Group repositories that ship one product."</p></div>
			{if organization.projects.is_empty() {
				view! { <p class="project-empty">"No projects yet. A project brings several of this organisation's repositories together under one name."</p> }.into_any()
			} else {
				view! {
					<ul class="project-grid">
						{organization.projects.iter().map(|project| {
							let href = project_path(&login, &project.slug);
							let count = match project.repository_count { 1 => "1 repository".to_string(), count => format!("{count} repositories") };
							view! {
								<li><a href=href class="project-card"><h3>{project.name.clone()}</h3><p>{if project.description.is_empty() { count.clone() } else { project.description.clone() }}</p><span>{count} <ArrowIcon /></span></a></li>
							}
						}).collect::<Vec<_>>()}
					</ul>
				}.into_any()
			}}
			{if organization.supports_projects {
				view! { <NewProjectForm organization=organization.clone() /> }.into_any()
			} else {
				view! { <p class="form-note">"monochange is still confirming this account with GitHub. Reload the page in a moment to create projects."</p> }.into_any()
			}}
		</section>
		<section class="repository-section" aria-labelledby="repositories-title">
			<div class="repository-heading"><h2 id="repositories-title">"Repositories"</h2><p>{organization.repositories.len()} " connected"</p></div>
			<ul class="repository-list">
				{organization.repositories.into_iter().map(|repo| view! {
					<li><div><h3>{repo.github_full_name}</h3><p>{if repo.github_private { "Private" } else { "Public" }}{if repo.installation_suspended { " · Installation suspended" } else { "" }}</p></div></li>
				}).collect::<Vec<_>>()}
			</ul>
		</section>
	}
}

/// Creates a project. Plain form fields, so it also works before the page
/// hydrates; the server validates everything again.
#[component]
fn NewProjectForm(organization: OrganizationSummary) -> impl IntoView {
	let create = ServerAction::<CreateProject>::new();
	let login = organization.login.clone();
	let example = project_path(&login, "invoices");

	view! {
		<details class="project-form-panel">
			<summary class="button button-brand">"New project"</summary>
			<ActionForm action=create attr:class="project-form">
				<input type="hidden" name="organization" value=login />
				<label class="field"><span>"Name"</span><input name="name" required maxlength=MAX_NAME_CHARS autocomplete="off" aria-describedby="project-address-hint" /></label>
				<p class="field-hint" id="project-address-hint">"Its address comes from the name: “Invoices” lives at " {example}</p>
				<label class="field"><span>"Description " <small>"(optional)"</small></span><textarea name="description" rows="2" maxlength=MAX_DESCRIPTION_CHARS></textarea></label>
				<fieldset class="field">
					<legend>"Repositories"</legend>
					<div class="checkbox-list">
						{organization.repositories.into_iter().map(|repo| view! {
							<label><input type="checkbox" name="repositories[]" value=repo.github_full_name.clone() /><span>{repo.github_full_name.clone()}</span><small>{if repo.github_private { "Private" } else { "Public" }}</small></label>
						}).collect::<Vec<_>>()}
					</div>
				</fieldset>
				{move || create.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
				<button type="submit" class="button button-brand" disabled=move || create.pending().get()>{move || if create.pending().get() { "Creating…" } else { "Create project" }}</button>
			</ActionForm>
		</details>
	}
}
