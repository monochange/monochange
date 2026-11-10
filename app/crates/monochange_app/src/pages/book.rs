//! The book inside the public website, with shared navigation and theme.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::LazyRoute;
use leptos_router::hooks::use_params_map;
use leptos_router::lazy_route;

use crate::error::ErrorTemplate;
use crate::server_fns::book::BookPage;
use crate::server_fns::book::get_book_page;

/// Book page data, split into its own WASM chunk.
pub struct BookRoute {
	chapter: Resource<Result<Option<BookPage>, server_fn::ServerFnError>>,
}

/// Read a chapter without leaving monochange.dev.
#[lazy_route]
impl LazyRoute for BookRoute {
	fn data() -> Self {
		let params = use_params_map();
		let chapter = Resource::new(
			move || params.get().get("chapter").unwrap_or_default(),
			get_book_page,
		);
		Self { chapter }
	}

	fn view(this: Self) -> AnyView {
		let BookRoute { chapter } = this;
		book_route_view(chapter)
	}
}

fn book_route_view(
	chapter: Resource<Result<Option<BookPage>, server_fn::ServerFnError>>,
) -> AnyView {
	view! {
		<div class="site-width book-layout">
			<Suspense fallback=|| view! { <p role="status">"Loading the book…"</p> }>
				{move || chapter.get().map(|result| view! { <BookPageView result=result /> })}
			</Suspense>
		</div>
	}
	.into_any()
}

#[component]
fn BookPageView(result: Result<Option<BookPage>, ServerFnError>) -> impl IntoView {
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

/// Book screen for `/book` and `/book/*`.
pub fn book_screen(chapter: Vec<String>) -> leptoskit::prelude::Element {
	let path = chapter.join("/");
	let key = crate::slots::once(|| RwSignal::new(path.clone()));
	key.set(path);
	let page = crate::slots::once(|| Resource::new_blocking(move || key.get(), get_book_page));

	match page.get() {
		None => crate::ui::text("Loading the book…"),
		Some(Err(_)) => crate::ui::text("The book is unavailable right now. Please try again."),
		Some(Ok(None)) => {
			#[cfg(not(target_arch = "wasm32"))]
			if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
				response.set_status(axum::http::StatusCode::NOT_FOUND);
			}

			crate::ui::heading(1, "Chapter not found")
		}
		Some(Ok(Some(page))) => book_content(page),
	}
}

fn book_content(page: BookPage) -> leptoskit::prelude::Element {
	let mut children = vec![
		crate::ui::link("/", "Back to monochange"),
		crate::ui::heading(1, page.chapter.title.clone()),
		crate::ui::text(format!("The monochange book · {}", page.chapter.section)),
	];

	for link in page.navigation {
		children.push(crate::ui::link(&link.href, &link.title));
	}

	if let Some(previous) = page.chapter.previous {
		children.push(crate::ui::link(
			&previous.href,
			&format!("Previous {}", previous.title),
		));
	}

	if let Some(next) = page.chapter.next {
		children.push(crate::ui::link(&next.href, &format!("Next {}", next.title)));
	}

	children.push(crate::ui::link(
		&format!(
			"https://github.com/monochange/monochange/blob/main/docs/src/{}",
			page.chapter.source
		),
		"Edit this chapter on GitHub",
	));
	// leptoskit 0.3 has no rich-text node, so the sanitized chapter is shown as text.
	children.push(crate::ui::text(plain_text(&page.chapter.html)));

	crate::ui::with_children(crate::ui::column("flex flex-col gap-3 p-6"), children)
}

fn plain_text(html: &str) -> String {
	let mut text = String::new();
	let mut inside = false;

	for character in html.chars() {
		match character {
			'<' => inside = true,
			'>' => {
				inside = false;
				text.push(' ');
			}
			_ if !inside => text.push(character),
			_ => {}
		}
	}

	text
}

#[cfg(test)]
#[path = "__tests__/book_tests.rs"]
mod tests;
