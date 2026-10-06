//! The book inside the public website, with shared navigation and theme.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

use crate::error::ErrorTemplate;
use crate::server_fns::book::BookPage;
use crate::server_fns::book::get_book_page;

/// Read a chapter without leaving monochange.dev.
#[component]
pub fn BookPageView() -> impl IntoView {
	let params = use_params_map();
	let chapter = Resource::new(
		move || params.get().get("chapter").unwrap_or_default(),
		get_book_page,
	);
	view! {
		<div class="site-width book-layout">
			<Suspense fallback=|| view! { <p role="status">"Loading the book…"</p> }>
				{move || chapter.get().map(book_page)}
			</Suspense>
		</div>
	}
}

fn book_page(result: Result<Option<BookPage>, server_fn::ServerFnError>) -> AnyView {
	match result {
		Err(_) => {
			view! { <p role="alert">"The book is unavailable right now. Please try again."</p> }
				.into_any()
		}
		Ok(None) => {
			#[cfg(not(target_arch = "wasm32"))]
			if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
				response.set_status(axum::http::StatusCode::NOT_FOUND);
			}
			view! { <ErrorTemplate status=404 message="Chapter not found" /> }.into_any()
		}
		Ok(Some(page)) => {
			let current = page.chapter.href.clone();
			let (chapters_open, set_chapters_open) = signal(false);
			view! {
				<Title text=format!("{} | monochange book", page.chapter.title) />
				<aside class="book-sidebar">
					<a class="book-home" href="/">"← Back to monochange.dev"</a>
					<div class="book-chapters">
						<button class="book-chapters-toggle" aria-expanded=move || chapters_open.get().to_string() aria-controls="book-chapter-navigation" on:click=move |_| set_chapters_open.update(|open| *open = !*open)>"Chapters"</button>
						<nav id="book-chapter-navigation" class=move || if chapters_open.get() { "book-chapter-navigation is-open" } else { "book-chapter-navigation" } aria-label="Book chapters">
							{page.navigation.into_iter().map(|link| {
								let active = current == link.href;
								view! { <a href=link.href aria-current=active.then_some("page")><span>{link.section}</span>{link.title}</a> }
							}).collect_view()}
						</nav>
					</div>
				</aside>
				<article class="book-article">
					<p class="eyebrow">"The monochange book · "{page.chapter.section}</p>
					<div class="book-prose" inner_html=page.chapter.html />
					<nav class="book-pagination" aria-label="Adjacent chapters">
						{page.chapter.previous.map(|link| view! { <a href=link.href><span>"Previous"</span>{link.title}</a> })}
						{page.chapter.next.map(|link| view! { <a href=link.href><span>"Next"</span>{link.title}</a> })}
					</nav>
					<a class="book-edit" href=format!("https://github.com/monochange/monochange/blob/main/docs/src/{}", page.chapter.source)>"Edit this chapter on GitHub"</a>
				</article>
			}.into_any()
		}
	}
}

#[cfg(test)]
#[path = "__tests__/book_tests.rs"]
mod tests;
