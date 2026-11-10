//! Public navigation with accessible mobile controls and persistent color mode.
//!
//! Marketing chrome uses Tailwind utilities + Release floor `@theme` tokens.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::color_mode::ColorMode;
use crate::color_mode::provide_color_mode;
use crate::color_mode::use_color_mode;
use crate::links::BOOK_URL;
use crate::server_fns::auth::Logout;
use crate::server_fns::auth::SessionUser;
use crate::server_fns::auth::get_session;
use crate::server_fns::portal::navigation_sign_in_path;

const SITE: &str = "mx-auto w-[min(80rem,calc(100%-6rem))] max-[1050px]:w-[calc(100%-4rem)] max-[760px]:w-[calc(100%-2.5rem)]";
const BTN_BRAND: &str = "inline-flex min-h-12 shrink-0 items-center justify-center gap-4 whitespace-nowrap rounded-xl bg-brand-700 px-[1.35rem] py-[0.85rem] font-bold leading-snug text-white no-underline transition-colors hover:bg-brand-800";

/// Collapse the mobile menu when Escape is pressed.
fn close_mobile_nav_on_key(key: &str, set_mobile_open: WriteSignal<bool>) {
	if key == "Escape" {
		set_mobile_open.set(false);
	}
}

/// Public navigation, including a direct link to the published book.
#[component]
pub fn NavBar() -> impl IntoView {
	let _ = provide_color_mode();
	let color_mode = use_color_mode();
	let (mobile_open, set_mobile_open) = signal(false);
	let location = use_location();
	let sign_out = ServerAction::<Logout>::new();
	let session = Resource::new_blocking(
		move || (location.pathname.get(), sign_out.version().get()),
		|_| get_session(),
	);

	Effect::new(move || {
		location.pathname.get();
		set_mobile_open.set(false);
	});

	let on_home = Memo::new(move |_| location.pathname.get() == "/");

	view! {
		<a href="#main-content" class="fixed left-4 top-2 z-[100] -translate-y-[150%] bg-surface px-4 py-3 focus:translate-y-0">
			"Skip to content"
		</a>
		<header class=move || {
			if on_home.get() {
				"sticky top-0 z-40 border-b border-transparent bg-hero/80 text-mist backdrop-blur-[10px]"
			} else {
				"sticky top-0 z-40 border-b border-rule bg-page"
			}
		}>
			<nav
				class=format!("{SITE} relative flex min-h-[88px] items-center justify-between gap-8 max-[900px]:min-h-[76px] max-[900px]:flex-wrap max-[900px]:gap-4")
				aria-label="Main navigation"
				on:keydown=move |event| close_mobile_nav_on_key(&event.key(), set_mobile_open)
			>
				<a href="/" class="inline-flex items-center gap-3 text-2xl font-bold tracking-[-0.035em] no-underline max-[900px]:gap-2.5 max-[900px]:text-[1.3rem]" aria-label="monochange home">
					<img src="/branding/mark.svg" width="36" height="36" alt="" class="shrink-0" />
					<img
						src="/branding/wordmark.svg"
						width="154"
						height="29"
						alt=""
						class=move || {
							if on_home.get() {
								"hidden h-auto w-[154px] max-[900px]:w-[138px]"
							} else {
								"h-auto w-[154px] max-[900px]:w-[138px] dark:hidden"
							}
						}
					/>
					<img
						src="/branding/wordmark-dark.svg"
						width="154"
						height="29"
						alt=""
						class=move || {
							if on_home.get() {
								"block h-auto w-[154px] max-[900px]:w-[138px]"
							} else {
								"hidden h-auto w-[154px] max-[900px]:w-[138px] dark:block"
							}
						}
					/>
				</a>
				<div class=move || {
					if on_home.get() {
						"hidden items-center gap-8 min-[901px]:flex [&_a]:py-[0.65rem] [&_a]:font-bold [&_a]:text-mist [&_a:hover]:text-brand-400 [&_a[aria-current=page]]:text-brand-400"
					} else {
						"hidden items-center gap-8 min-[901px]:flex [&_a]:py-[0.65rem] [&_a]:font-bold [&_a:hover]:text-brand-600 [&_a[aria-current=page]]:text-brand-600 dark:[&_a:hover]:text-brand-400 dark:[&_a[aria-current=page]]:text-brand-400"
					}
				}>
					<a href="/install" aria-current=move || (location.pathname.get() == "/install").then_some("page")>"Install"</a>
					<a href=BOOK_URL aria-current=move || location.pathname.get().starts_with(BOOK_URL).then_some("page")>"Docs"</a>
					<a href="/changelog" aria-current=move || (location.pathname.get() == "/changelog").then_some("page")>"Changelog"</a>
					<a href="/pricing" aria-current=move || (location.pathname.get() == "/pricing").then_some("page")>"Pricing"</a>
				</div>
				<div class="flex items-center gap-4 max-[900px]:gap-1">
					<button
						class=move || {
							if on_home.get() {
								"grid min-h-11 min-w-11 place-items-center rounded-[10px] hover:bg-mist/10"
							} else {
								"grid min-h-11 min-w-11 place-items-center rounded-[10px] hover:bg-surface"
							}
						}
						on:click=move |_| color_mode.toggle.run(())
						aria-label=move || if color_mode.mode.get() == ColorMode::Dark { "Use light theme" } else { "Use dark theme" }
					>
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" width="20" height="20" aria-hidden="true" class:hidden=move || color_mode.mode.get() == ColorMode::Dark>
							<path d="M20.5 14.3A9 9 0 0 1 9.7 3.5 9 9 0 1 0 20.5 14.3Z" />
						</svg>
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" width="20" height="20" aria-hidden="true" class:hidden=move || color_mode.mode.get() == ColorMode::Light>
							<circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5M17.5 17.5L19 19M5 19l1.5-1.5M17.5 6.5L19 5" />
						</svg>
					</button>
					<div class="hidden items-center gap-4 min-[901px]:flex"><SessionLinks session=session sign_out=sign_out /></div>
					<button
						class=move || {
							if on_home.get() {
								"grid min-h-11 min-w-11 place-items-center rounded-[10px] hover:bg-mist/10 min-[901px]:hidden"
							} else {
								"grid min-h-11 min-w-11 place-items-center rounded-[10px] hover:bg-surface min-[901px]:hidden"
							}
						}
						aria-label="Toggle navigation menu"
						aria-expanded=move || mobile_open.get().to_string()
						aria-controls="mobile-navigation"
						on:click=move |_| set_mobile_open.update(|open| *open = !*open)
					>
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" width="22" height="22" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16" /></svg>
					</button>
				</div>
				<div
					id="mobile-navigation"
					class="flex w-full flex-col pb-4 min-[901px]:hidden"
					hidden=move || !mobile_open.get()
				>
					<a href="/install" class="min-h-11 border-t border-rule px-4 py-2.5">"Install"</a>
					<a href=BOOK_URL class="min-h-11 border-t border-rule px-4 py-2.5">"Docs"</a>
					<a href="/changelog" class="min-h-11 border-t border-rule px-4 py-2.5">"Changelog"</a>
					<a href="/pricing" class="min-h-11 border-t border-rule px-4 py-2.5">"Pricing"</a>
					<SessionLinks session=session sign_out=sign_out />
				</div>
			</nav>
		</header>
	}
}

#[component]
fn SessionLinks(
	session: Resource<Result<Option<SessionUser>, server_fn::ServerFnError>>,
	sign_out: ServerAction<Logout>,
) -> impl IntoView {
	let location = use_location();
	let sign_in = move || navigation_sign_in_path(&location.pathname.get());
	view! {
		<Suspense fallback=|| view! { <span role="status">"Loading account…"</span> }>
			{move || session.get().map(|result| match result {
				Ok(Some(user)) => view! {
					// /projects belongs to the projects browser entry; the link
					// must load that entry as a document navigation.
					<a
						href="/projects"
						target=monochange_shared::routing::cross_surface_target("/", "/projects")
						class=BTN_BRAND
						title=format!("Signed in as {}", user.github_login)
					>
						"Projects"
					</a>
					<ActionForm action=sign_out>
						<button type="submit" class="inline-block py-[0.65rem] font-bold text-brand-600 underline dark:text-brand-400" disabled=move || sign_out.pending().get()>"Sign out"</button>
					</ActionForm>
				}.into_any(),
				Ok(None) => view! { <a href=sign_in() class=BTN_BRAND>"Sign in"</a> }.into_any(),
				Err(_) => view! { <a href=sign_in() class=BTN_BRAND>"Sign in again"</a> }.into_any(),
			})}

		</Suspense>
	}
}

#[cfg(test)]
#[path = "__tests__/navbar_tests.rs"]
mod tests;
