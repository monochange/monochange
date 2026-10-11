//! One project: the repositories it brings together, and its settings.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

use crate::pages::organization::Unavailable;
use crate::pages::organization::action_error;
use crate::projects::MAX_DESCRIPTION_CHARS;
use crate::projects::MAX_NAME_CHARS;
use crate::server_fns::organizations::DeleteProject;
use crate::server_fns::organizations::ProjectDetails;
use crate::server_fns::organizations::UpdateProject;
use crate::server_fns::organizations::organization_path;
use crate::server_fns::organizations::project_overview;

#[component]
pub fn ProjectPage() -> impl IntoView {
	let params = use_params_map();
	let key = move || {
		let params = params.read();
		(
			params.get("organization").unwrap_or_default(),
			params.get("project").unwrap_or_default(),
		)
	};
	let details = Resource::new_blocking(key, |(organization, project)| {
		project_overview(organization, project)
	});

	view! {
		<Suspense fallback=|| view! { <p class="site-width dashboard-section" role="status">"Loading project…"</p> }>
			{move || details.get().map(|result| match result {
				Ok(Some(details)) => view! { <ProjectContent details=details /> }.into_any(),
				Ok(None) => view! { <section class="site-width"><Unavailable title="Project not found" message="It may have been renamed or deleted, or it belongs to an organisation you don't own." /></section> }.into_any(),
				Err(_) => view! { <section class="site-width"><Unavailable title="This project couldn't be loaded" message="Please reload the page to try again." /></section> }.into_any(),
			})}
		</Suspense>
	}
}

#[component]
fn ProjectContent(details: ProjectDetails) -> impl IntoView {
	let organization = details.organization.clone();
	let project = details.project.clone();
	let disconnected = details
		.repositories
		.iter()
		.filter(|repository| repository.private.is_none())
		.count();

	view! {
		<Title text=format!("{} — monochange", project.name) />
		<section class="site-width dashboard-section">
			<nav class="breadcrumbs" aria-label="Breadcrumb">
				<a href="/dashboard">"Workspace"</a><span aria-hidden="true">"/"</span>
				<a href=organization_path(&organization)>{organization.clone()}</a><span aria-hidden="true">"/"</span>
				<span aria-current="page">{project.name.clone()}</span>
			</nav>
			<div class="project-heading">
				<h1>{project.name.clone()}</h1>
				{(!project.description.is_empty()).then(|| view! { <p>{project.description.clone()}</p> })}
			</div>
			<section class="repository-section" aria-labelledby="project-repositories-title">
				<div class="repository-heading"><h2 id="project-repositories-title">"Repositories"</h2><p>{details.repositories.len()} " in this project"</p></div>
				{(disconnected > 0).then(|| view! {
					<p class="form-note">"Disconnected repositories stay in the project and come back when the GitHub App is installed on them again."</p>
				})}
				<ul class="repository-list">
					{details.repositories.iter().map(|repository| {
						let state = match repository.private {
							Some(true) => "Private",
							Some(false) => "Public",
							None => "Disconnected",
						};
						view! { <li><div><h3>{repository.full_name.clone()}</h3><p>{state}</p></div></li> }
					}).collect::<Vec<_>>()}
				</ul>
			</section>
			<section class="repository-section" aria-labelledby="project-settings-title">
				<div class="repository-heading"><h2 id="project-settings-title">"Settings"</h2></div>
				<EditProjectForm details=details.clone() />
				<DeleteProjectForm organization=organization project=project.slug.clone() />
			</section>
		</section>
	}
}

#[component]
fn EditProjectForm(details: ProjectDetails) -> impl IntoView {
	let update = ServerAction::<UpdateProject>::new();
	let selected: Vec<String> = details
		.repositories
		.iter()
		.map(|repository| repository.full_name.to_ascii_lowercase())
		.collect();

	view! {
		<details class="project-form-panel">
			<summary class="button button-light">"Edit project"</summary>
			<ActionForm action=update attr:class="project-form">
				<input type="hidden" name="organization" value=details.organization.clone() />
				<input type="hidden" name="project" value=details.project.slug.clone() />
				<label class="field"><span>"Name"</span><input name="name" required maxlength=MAX_NAME_CHARS value=details.project.name.clone() autocomplete="off" /></label>
				<label class="field"><span>"Description " <small>"(optional)"</small></span><textarea name="description" rows="2" maxlength=MAX_DESCRIPTION_CHARS>{details.project.description.clone()}</textarea></label>
				<fieldset class="field">
					<legend>"Repositories"</legend>
					<div class="checkbox-list">
						{details.available.into_iter().map(|repo| {
							let checked = selected.contains(&repo.github_full_name.to_ascii_lowercase());
							view! {
								<label><input type="checkbox" name="repositories[]" value=repo.github_full_name.clone() checked=checked /><span>{repo.github_full_name.clone()}</span><small>{if repo.github_private { "Private" } else { "Public" }}</small></label>
							}
						}).collect::<Vec<_>>()}
					</div>
				</fieldset>
				{move || update.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
				<button type="submit" class="button button-brand" disabled=move || update.pending().get()>"Save changes"</button>
			</ActionForm>
		</details>
	}
}

#[component]
fn DeleteProjectForm(organization: String, project: String) -> impl IntoView {
	let delete = ServerAction::<DeleteProject>::new();
	view! {
		<details class="project-form-panel danger-zone">
			<summary class="text-link">"Delete project"</summary>
			<ActionForm action=delete attr:class="project-form">
				<input type="hidden" name="organization" value=organization />
				<input type="hidden" name="project" value=project />
				<p>"Deleting a project removes it from monochange. Its repositories stay connected and nothing changes on GitHub."</p>
				{move || delete.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
				<button type="submit" class="button button-danger" disabled=move || delete.pending().get()>"Delete this project"</button>
			</ActionForm>
		</details>
	}
}
