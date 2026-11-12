//! One organisation: its projects, a form to create one, and its connected
//! repositories.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::LazyRoute;
use leptos_router::hooks::use_params_map;
use leptos_router::lazy_route;
use monochange_projects::components::arrow::ArrowIcon;
use monochange_projects::components::repository_list::RepositoryFilters;
use monochange_projects::components::repository_list::RepositoryPager;
use monochange_projects::components::repository_list::RepositoryPicker;
use monochange_projects::components::repository_list::use_repository_filter;
use monochange_projects::error::action_error;
use monochange_projects::pages::home::OrganizationAvatar;
use monochange_projects::projects::MAX_DESCRIPTION_CHARS;
use monochange_projects::projects::MAX_NAME_CHARS;
use monochange_projects::repository_filter::count_matches;
use monochange_projects::repository_filter::repository_count;
use monochange_projects::server_fns::organizations::CreateProject;
use monochange_projects::server_fns::organizations::OrganizationSummary;
use monochange_projects::server_fns::organizations::organization_overview;
use monochange_projects::server_fns::organizations::organization_path;
use monochange_projects::server_fns::organizations::project_path;

use crate::resource_codec::JsonSliceCodec;

/// One-organisation page data, split into its own WASM chunk.
pub struct OrganizationRoute {
	login: Memo<String>,
	overview:
		Resource<Result<Option<OrganizationSummary>, server_fn::ServerFnError>, JsonSliceCodec>,
}

/// One organisation: its projects, a form to create one, and its connected
/// repositories.
#[lazy_route]
impl LazyRoute for OrganizationRoute {
	fn data() -> Self {
		let params = use_params_map();
		let login = Memo::new(move |_| params.read().get("organization").unwrap_or_default());
		let overview = Resource::new_with_options(move || login.get(), organization_overview, true);
		Self { login, overview }
	}

	fn view(this: Self) -> AnyView {
		let OrganizationRoute { login, overview } = this;
		view! {
			<Title text=move || format!("{} — monochange", login.get()) />
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
		.into_any()
	}
}

/// A not-found or failure state with a way back.
#[component]
pub fn Unavailable(title: &'static str, message: &'static str) -> impl IntoView {
	view! {
		<div class="status-page">
			<div><h1>{title}</h1><p>{message}</p><div class="actions"><a href="/projects" class="button button-brand">"Back to your workspace"</a></div></div>
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
	let filter = use_repository_filter();
	let total = organization.repositories.len();
	let searchable = StoredValue::new(organization.repositories.clone());
	view! {
		<nav class="breadcrumbs" aria-label="Breadcrumb"><a href="/projects">"Workspace"</a><span aria-hidden="true">"/"</span><span aria-current="page">{login.clone()}</span></nav>
		<div class="workspace-heading">
			<OrganizationAvatar login=login.clone() avatar_url=organization.avatar_url.clone() />
			<div><h1>{login.clone()}</h1><p>{kind} " · " {format!("{} connected", repository_count(organization.repositories.len()))}</p></div>
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
		<section class="repository-section" id="repositories" aria-labelledby="repositories-title">
			<div class="repository-heading"><h2 id="repositories-title">"Repositories"</h2><p aria-live="polite">{move || {
				let search = filter.get().search;
				if search.trim().is_empty() { format!("{total} connected") } else { format!("{} of {total} match", count_matches(&searchable.read_value(), &search)) }
			}}</p></div>
			<RepositoryFilters filter=filter path=organization_path(&login) organizations=Vec::new() />
			<RepositoryPager repositories=organization.repositories search=Signal::derive(move || filter.get().search) label=format!("{login} repositories") />
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
				<RepositoryPicker repositories=organization.repositories />
				{move || create.value().get().and_then(Result::err).map(|error| view! { <p class="form-error" role="alert">{action_error(&error)}</p> })}
				<button type="submit" class="button button-brand" disabled=move || create.pending().get()>{move || if create.pending().get() { "Creating…" } else { "Create project" }}</button>
			</ActionForm>
		</details>
	}
}

#[cfg(test)]
#[path = "__tests__/organization_tests.rs"]
mod tests;
