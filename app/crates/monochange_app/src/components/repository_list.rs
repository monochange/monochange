//! Long repository lists made manageable: a name search and account filter
//! kept in the URL, pages within each list, and a filterable picker for
//! project forms.

use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_query_map;

use crate::repository_filter::ORGANIZATION_KEY;
use crate::repository_filter::RepositoryFilter;
use crate::repository_filter::SEARCH_KEY;
use crate::repository_filter::matches_search;
use crate::repository_filter::repository_page;
use crate::server_fns::repos::RepoInfo;

/// Pickers with more repositories than this get a filter field.
pub const PICKER_FILTER_THRESHOLD: usize = 8;

/// A form id no element uses. The picker's filter field names it, so the
/// field belongs to no form: Enter in it can't submit the project form, and
/// its text is never sent.
pub const PICKER_FILTER_FORM: &str = "repository-picker-filter";

/// The repository filters in the current page's query string.
pub fn use_repository_filter() -> Memo<RepositoryFilter> {
	let query = use_query_map();
	Memo::new(move |_| {
		query
			.with(|query| RepositoryFilter::new(query.get(SEARCH_KEY), query.get(ORGANIZATION_KEY)))
	})
}

/// Decorative magnifier for search fields.
#[component]
pub fn SearchIcon() -> impl IntoView {
	view! {
		<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
	}
}

/// Decorative chevron that turns as a section opens.
#[component]
pub fn ChevronIcon() -> impl IntoView {
	view! {
		<svg class="chevron" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
	}
}

/// A name search, plus an account filter when there is more than one
/// account. It is a plain GET form, so it works before the page hydrates;
/// afterwards each change updates the URL in place. The visually hidden
/// submit button keeps Enter working without JavaScript. It isn't wrapped in
/// `<noscript>`, whose contents a scripting browser parses as text, which
/// would break hydration.
#[component]
pub fn RepositoryFilters(
	filter: Memo<RepositoryFilter>,
	/// The page the filters belong to, such as `/dashboard`.
	path: String,
	/// Accounts to choose from; empty hides the account filter.
	organizations: Vec<String>,
) -> impl IntoView {
	let navigate = use_navigate();
	let target = path.clone();
	// patch-coverage:ignore-start -- navigation only runs in the browser after an input event; the URL it builds is `RepositoryFilter::href`, covered by the filter tests.
	let apply = move |next: RepositoryFilter| {
		let options = NavigateOptions {
			replace: true,
			scroll: false,
			..NavigateOptions::default()
		};
		navigate(&next.href(&target), options);
	};
	// patch-coverage:ignore-end
	let apply_organization = apply.clone();

	view! {
		<form method="get" action=path role="search" class="repository-filters" on:submit=|event| event.prevent_default()>
			<label class="filter-search">
				<span class="sr-only">"Search repositories by name"</span>
				<SearchIcon />
				<input type="search" name=SEARCH_KEY placeholder="Search repositories" autocomplete="off" spellcheck="false" value=filter.get_untracked().search prop:value=move || filter.get().search on:input=move |event| apply(filter.get_untracked().with_search(event_target_value(&event))) />
			</label>
			{(organizations.len() > 1).then(|| view! {
				<label class="filter-account">
					<span class="sr-only">"Account"</span>
					<select name=ORGANIZATION_KEY on:change=move |event| apply_organization(filter.get_untracked().with_organization(event_target_value(&event)))>
						<option value="" selected=move || filter.get().organization.is_none()>"All accounts"</option>
						{organizations.into_iter().map(|login| {
							let value = login.clone();
							view! { <option value=value.clone() selected=move || filter.get().organization.is_some_and(|selected| selected.eq_ignore_ascii_case(&value))>{login}</option> }
						}).collect::<Vec<_>>()}
					</select>
					<ChevronIcon />
				</label>
			})}
			<button type="submit" class="sr-only">"Search"</button>
		</form>
	}
}

/// One repository: its name, and its visibility as a chip.
#[component]
fn RepositoryRow(repository: RepoInfo) -> impl IntoView {
	let name = repository
		.github_full_name
		.rsplit_once('/')
		.map_or(repository.github_full_name.clone(), |(_, name)| {
			name.to_owned()
		});
	let visibility = if repository.github_private {
		"Private"
	} else {
		"Public"
	};

	view! {
		<li>
			<div><h3 title=repository.github_full_name.clone()>{name}</h3>{repository.installation_suspended.then(|| view! { <p>"Installation suspended"</p> })}</div>
			<span class="chip">{visibility}</span>
		</li>
	}
}

/// The repositories that match `search`, a page at a time.
#[component]
pub fn RepositoryPager(
	repositories: Vec<RepoInfo>,
	#[prop(into)] search: Signal<String>,
	/// Names the list for assistive technology, such as "acme repositories".
	label: String,
) -> impl IntoView {
	let repositories = StoredValue::new(repositories);
	// The page belongs to the search it was chosen for; a new search starts
	// again from the first page.
	let requested = RwSignal::new((String::new(), 1_usize));
	let current = move || {
		let search = search.get();
		let (chosen_for, page) = requested.get();
		let page = if chosen_for == search { page } else { 1 };
		repositories.with_value(|repositories| repository_page(repositories, &search, page))
	};
	let go = move |page: usize| requested.set((search.get_untracked(), page));

	view! {
		{move || {
			let page = current();

			if page.matches == 0 {
				return view! { <p class="repository-none">"No repositories match “" {search.get().trim().to_owned()} "”."</p> }.into_any();
			}

			let (previous, next) = (page.page - 1, page.page + 1);
			let (has_previous, has_next) = (page.has_previous(), page.has_next());
			view! {
				<ul class="repository-list repository-scroll" aria-label=label.clone()>
					{page.repositories.iter().cloned().map(|repository| view! { <RepositoryRow repository=repository /> }).collect::<Vec<_>>()}
				</ul>
				{(page.page_count > 1).then(|| view! {
					<nav class="pager" aria-label=format!("{label} pages")>
						<p>"Showing " {page.first} "–" {page.last} " of " {page.matches}</p>
						<div>
							<button type="button" class="pager-button" disabled=!has_previous on:click=move |_| go(previous)>"Previous"</button>
							<span>"Page " {page.page} " of " {page.page_count}</span>
							<button type="button" class="pager-button" disabled=!has_next on:click=move |_| go(next)>"Next"</button>
						</div>
					</nav>
				})}
			}
			.into_any()
		}}
	}
}

/// Project-form checkboxes for `repositories`, with a filter for long lists.
/// Filtered-out repositories are only hidden, so they keep their selection
/// and are still submitted.
#[component]
pub fn RepositoryPicker(
	repositories: Vec<RepoInfo>,
	/// Lower-case full names that start checked.
	#[prop(optional)]
	selected: Vec<String>,
) -> impl IntoView {
	let (search, set_search) = signal(String::new());
	let filterable = repositories.len() > PICKER_FILTER_THRESHOLD;
	let all = StoredValue::new(repositories.clone());
	let none_match = move || {
		all.with_value(|repositories| {
			!repositories
				.iter()
				.any(|repository| matches_search(repository, &search.read()))
		})
	};

	view! {
		<fieldset class="field">
			<legend>"Repositories"</legend>
			{filterable.then(|| view! {
				<label class="filter-search picker-search">
					<span class="sr-only">"Filter repositories by name"</span>
					<SearchIcon />
					<input type="search" form=PICKER_FILTER_FORM placeholder="Filter repositories" autocomplete="off" spellcheck="false" on:input=move |event| set_search.set(event_target_value(&event)) />
				</label>
			})}
			<div class="checkbox-list">
				{repositories.into_iter().map(move |repository| {
					let checked = selected.contains(&repository.github_full_name.to_ascii_lowercase());
					let visibility = if repository.github_private { "Private" } else { "Public" };
					let full_name = repository.github_full_name.clone();
					view! {
						<label hidden=move || !matches_search(&repository, &search.read())><input type="checkbox" name="repositories[]" value=full_name.clone() checked=checked /><span>{full_name.clone()}</span><small>{visibility}</small></label>
					}
				}).collect::<Vec<_>>()}
				<p class="repository-none" hidden=move || !none_match()>"No repositories match."</p>
			</div>
		</fieldset>
	}
}

#[cfg(test)]
#[path = "__tests__/repository_list_tests.rs"]
mod tests;
