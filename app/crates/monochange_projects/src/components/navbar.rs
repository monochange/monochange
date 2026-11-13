//! Public navigation with accessible mobile controls and persistent color mode.
//!
//! Both browser entries render this one header, so the website and the
//! projects workspace share identical chrome: the site sections in the
//! middle, and one primary action plus one account menu on the right.
//! Marketing chrome uses Tailwind utilities + Release floor `@theme` tokens.

use leptos::html::Details;
use leptos::html::Summary;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use monochange_shared::routing::cross_surface_target;
use monochange_shared::routing::is_projects_path;

use crate::color_mode::ColorMode;
use crate::color_mode::provide_color_mode;
use crate::color_mode::use_color_mode;
use crate::server_fns::auth::Logout;
use crate::server_fns::auth::SessionUser;
use crate::server_fns::auth::get_session;
use crate::server_fns::portal::navigation_sign_in_path;

const SITE: &str = "mx-auto w-[min(80rem,calc(100%-6rem))] max-[1050px]:w-[calc(100%-4rem)] max-[760px]:w-[calc(100%-2.5rem)]";
const BOOK_URL: &str = "/book";
const BUTTON: &str = "inline-flex min-h-12 shrink-0 items-center justify-center whitespace-nowrap rounded-xl px-[1.35rem] py-[0.85rem] font-bold leading-snug no-underline transition-colors";
/// The header's one filled action: sign in, or open your projects.
const BUTTON_BRAND: &str = "bg-brand-700 text-white hover:bg-brand-800";
/// The projects action while the visitor is already in the workspace.
const BUTTON_CURRENT: &str = "bg-brand-100 text-brand-800 hover:bg-brand-200 dark:bg-brand-900 dark:text-brand-100 dark:hover:bg-brand-800";
/// One row in the account menu. Rows inherit the menu's text colour, so the
/// same hover tint works in the dropdown and on the homepage's mobile panel.
const MENU_ROW: &str = "flex min-h-11 w-full items-center rounded-[10px] px-3 text-left font-bold no-underline transition-colors hover:bg-current/10";
/// The account toggle: an avatar pill on wide screens, a full-width row naming
/// the account in the mobile panel. Only the wide pill draws the outside-click layer.
const ACCOUNT_TOGGLE: &str = "flex min-h-11 cursor-pointer list-none items-center gap-1.5 rounded-full border border-current/25 p-1 pr-2 transition-colors hover:border-brand-500 [&::-webkit-details-marker]:hidden min-[1101px]:group-open:before:fixed min-[1101px]:group-open:before:inset-0 min-[1101px]:group-open:before:cursor-default max-[1100px]:gap-3 max-[1100px]:rounded-[10px] max-[1100px]:border-0 max-[1100px]:px-3 max-[1100px]:py-1.5 max-[1100px]:font-bold max-[1100px]:hover:bg-current/10";
/// Buttons in the mobile panel span its width below the site sections.
const MOBILE_BUTTON: &str = "max-[1100px]:mt-4 max-[1100px]:w-full";

/// The site sections every page links to, in header order.
const SECTIONS: [(&str, &str); 4] = [
	("/install", "Install"),
	(BOOK_URL, "Docs"),
	("/changelog", "Changelog"),
	("/pricing", "Pricing"),
];

/// One account menu's disclosure and its toggle, so Escape and route changes
/// can close it and hand focus back. The header renders one menu for wide
/// screens and one inside the mobile panel.
#[derive(Clone, Copy)]
struct AccountMenu {
	menu: NodeRef<Details>,
	toggle: NodeRef<Summary>,
}

impl AccountMenu {
	fn new() -> Self {
		Self {
			menu: NodeRef::new(),
			toggle: NodeRef::new(),
		}
	}

	/// Close the menu if it is open, optionally returning focus to its toggle.
	fn close(self, restore_focus: bool) {
		// patch-coverage:ignore-start -- the menu only exists in a hydrated browser; server renders have no element to close.
		let Some(menu) = self.menu.get_untracked() else {
			return;
		};

		if !menu.has_attribute("open") {
			return;
		}

		let _ = menu.remove_attribute("open");

		if let Some(toggle) = self.toggle.get_untracked().filter(|_| restore_focus) {
			let _ = toggle.focus();
		}
		// patch-coverage:ignore-end
	}
}

/// Collapse the mobile panel and both account menus. Escape hands focus back
/// to the wide-screen toggle, whose menu hides under it; a route change and
/// the mobile panel's own toggle hide without moving focus.
fn close_menus(
	set_mobile_open: WriteSignal<bool>,
	desktop: AccountMenu,
	mobile: AccountMenu,
	restore_focus: bool,
) {
	set_mobile_open.set(false);
	desktop.close(restore_focus);
	mobile.close(false);
}

/// Collapse the mobile panel and both account menus when Escape is pressed.
/// Focus returns to the wide-screen toggle; the mobile one hides with its panel.
fn close_menus_on_key(
	key: &str,
	set_mobile_open: WriteSignal<bool>,
	desktop: AccountMenu,
	mobile: AccountMenu,
) {
	if key == "Escape" {
		close_menus(set_mobile_open, desktop, mobile, true);
	}
}

/// Public navigation, including a direct link to the published book.
#[component]
pub fn NavBar() -> impl IntoView {
	let _ = provide_color_mode();
	let color_mode = use_color_mode();
	let (mobile_open, set_mobile_open) = signal(false);
	let desktop_menu = AccountMenu::new();
	let mobile_menu = AccountMenu::new();
	let location = use_location();
	let sign_out = ServerAction::<Logout>::new();
	let session = Resource::new_blocking(
		move || (location.pathname.get(), sign_out.version().get()),
		|_| get_session(),
	);

	// A route change collapses the panel and both menus exactly like Escape,
	// without moving focus. Effects only run in the browser after hydration,
	// so like `color_mode` the effect is created only in the browser build;
	// the collapse contract itself is `close_menus`, covered on the server.
	#[cfg(target_arch = "wasm32")]
	Effect::new(move || {
		location.pathname.get();
		close_menus(set_mobile_open, desktop_menu, mobile_menu, false);
	});

	let on_home = Memo::new(move |_| location.pathname.get() == "/");
	let target = move |href: &str| cross_surface_target(&location.pathname.get(), href);
	let control = move || {
		if on_home.get() {
			"grid min-h-11 min-w-11 place-items-center rounded-[10px] hover:bg-mist/10"
		} else {
			"grid min-h-11 min-w-11 place-items-center rounded-[10px] hover:bg-surface"
		}
	};

	view! {
		<a href="#main-content" class="fixed left-4 top-2 z-[100] -translate-y-[150%] rounded-[10px] bg-surface px-4 py-3 font-bold text-ink focus:translate-y-0">
			"Skip to content"
		</a>
		<header class=move || {
			if on_home.get() {
				"sticky top-0 z-40 border-b border-transparent bg-hero text-mist"
			} else {
				"sticky top-0 z-40 border-b border-rule bg-page"
			}
		}>
			<nav
				class=format!("{SITE} relative flex min-h-[88px] items-center justify-between gap-8 max-[1100px]:min-h-[76px] max-[1100px]:flex-wrap max-[1100px]:gap-x-4 max-[1100px]:gap-y-0")
				aria-label="Main navigation"
				on:keydown=move |event| close_menus_on_key(&event.key(), set_mobile_open, desktop_menu, mobile_menu)
			>
				<a href="/" target=move || target("/") class="inline-flex items-center gap-3 text-2xl font-bold tracking-[-0.035em] no-underline max-[1100px]:gap-2.5 max-[1100px]:text-[1.3rem]" aria-label="monochange home">
					<img src="/branding/mark.svg" width="36" height="36" alt="" class="shrink-0" />
					<img
						src="/branding/wordmark.svg"
						width="154"
						height="29"
						alt=""
						class=move || {
							if on_home.get() {
								"hidden h-auto w-[154px] max-[1100px]:w-[138px]"
							} else {
								"h-auto w-[154px] max-[1100px]:w-[138px] dark:hidden"
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
								"block h-auto w-[154px] max-[1100px]:w-[138px]"
							} else {
								"hidden h-auto w-[154px] max-[1100px]:w-[138px] dark:block"
							}
						}
					/>
				</a>
				<div class=move || {
					if on_home.get() {
						"hidden items-center gap-8 min-[1101px]:flex [&_a]:py-[0.65rem] [&_a]:font-bold [&_a]:text-mist [&_a:hover]:text-brand-400 [&_a[aria-current=page]]:text-brand-400"
					} else {
						"hidden items-center gap-8 min-[1101px]:flex [&_a]:py-[0.65rem] [&_a]:font-bold [&_a:hover]:text-brand-600 [&_a[aria-current=page]]:text-brand-600 dark:[&_a:hover]:text-brand-400 dark:[&_a[aria-current=page]]:text-brand-400"
					}
				}>
					<SectionLinks />
				</div>
				<div class="flex items-center gap-3 max-[1100px]:gap-1">
					<button
						class=control
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
					<div class="hidden items-center gap-3 min-[1101px]:flex">
						<SessionControls session=session sign_out=sign_out menu=desktop_menu />
					</div>
					<button
						class=move || format!("{} min-[1101px]:hidden", control())
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
					class="flex w-full flex-col border-t border-current/15 pb-5 pt-3 min-[1101px]:hidden"
					hidden=move || !mobile_open.get()
				>
					<div class=move || {
						if on_home.get() {
							"flex flex-col gap-0.5 [&>a]:flex [&>a]:min-h-11 [&>a]:items-center [&>a]:rounded-[10px] [&>a]:px-3 [&>a]:font-bold [&>a:hover]:bg-current/10 [&>a[aria-current=page]]:text-brand-400"
						} else {
							"flex flex-col gap-0.5 [&>a]:flex [&>a]:min-h-11 [&>a]:items-center [&>a]:rounded-[10px] [&>a]:px-3 [&>a]:font-bold [&>a:hover]:bg-current/10 [&>a[aria-current=page]]:text-brand-700 dark:[&>a[aria-current=page]]:text-brand-400"
						}
					}>
						<SectionLinks />
					</div>
					<SessionControls session=session sign_out=sign_out menu=mobile_menu />
				</div>
			</nav>
		</header>
	}
}

/// The site sections as links, marking the current one. The wide header and
/// the mobile panel style them from their containers.
#[component]
fn SectionLinks() -> impl IntoView {
	let location = use_location();

	SECTIONS.map(|(href, label)| {
		view! {
			<a href=href target=move || cross_surface_target(&location.pathname.get(), href) aria-current=move || section_is_current(&location.pathname.get(), href).then_some("page")>{label}</a>
		}
	})
}

/// Whether `pathname` is inside the site section that `href` links to.
fn section_is_current(pathname: &str, href: &str) -> bool {
	if href == BOOK_URL {
		pathname.starts_with(BOOK_URL)
	} else {
		pathname == href
	}
}

/// The session-dependent end of the header. A transition keeps the current
/// controls on screen while the session reloads after each navigation, so
/// the header never flashes a loading state between pages.
///
/// The wide header and the mobile panel each render one copy. Each copy is
/// only displayed on its side of the 1100px breakpoint, so breakpoint
/// variants in one set of classes give each copy its own layout.
#[component]
fn SessionControls(
	session: Resource<Result<Option<SessionUser>, server_fn::ServerFnError>>,
	sign_out: ServerAction<Logout>,
	menu: AccountMenu,
) -> impl IntoView {
	let location = use_location();
	let sign_in = move |label: &'static str| {
		let href = navigation_sign_in_path(&location.pathname.get());
		let target = cross_surface_target(&location.pathname.get(), &href);

		view! { <a href=href target=target class=format!("{BUTTON} {BUTTON_BRAND} {MOBILE_BUTTON}")>{label}</a> }
	};

	view! {
		<Transition fallback=|| view! { <span class="sr-only" role="status">"Loading account…"</span> }>
			{move || session.map(|result| match result {
				Ok(Some(user)) => view! { <AccountControls user=user.clone() sign_out=sign_out menu=menu /> }.into_any(),
				Ok(None) => sign_in("Sign in").into_any(),
				Err(_) => sign_in("Sign in again").into_any(),
			})}

		</Transition>
	}
}

/// The workspace link: the header's primary action, tinted as the current
/// page while the visitor is already in the workspace. In the mobile panel
/// it closes the list as a full-width button.
#[component]
fn ProjectsLink() -> impl IntoView {
	let location = use_location();
	let current = move || is_projects_path(&location.pathname.get());
	let class = move || {
		let state = if current() {
			BUTTON_CURRENT
		} else {
			BUTTON_BRAND
		};

		format!("{BUTTON} {state} {MOBILE_BUTTON} max-[1100px]:order-last")
	};

	view! {
		// /projects belongs to the projects browser entry; the link must load
		// that entry as a document navigation.
		<a href="/projects" target=cross_surface_target("/", "/projects") class=class aria-current=move || current().then_some("page")>"Projects"</a>
	}
}

/// The signed-in visitor's GitHub avatar, or their initial without one.
#[component]
fn UserAvatar(user: SessionUser) -> impl IntoView {
	if let Some(url) = user.github_avatar_url {
		return view! { <img src=url alt="" width="32" height="32" class="size-8 shrink-0 rounded-full bg-page object-cover" /> }.into_any();
	}

	let initial = user
		.github_login
		.chars()
		.next()
		.map_or('?', |character| character.to_ascii_uppercase());

	view! {
		<span class="grid size-8 shrink-0 place-items-center rounded-full bg-brand-100 font-display text-[0.95rem] font-extrabold text-brand-800" aria-hidden="true">
			{initial.to_string()}
		</span>
	}
	.into_any()
}

/// Account pages that live on the website entry, in menu order.
const ACCOUNT_LINKS: [(&str, &str); 2] = [
	("/dashboard", "Connect repositories"),
	("/account", "Account and devices"),
];

/// The projects action and a disclosure menu for the account. On wide
/// screens the toggle is an avatar pill and the menu drops down; the open
/// menu draws a transparent full-viewport layer from its toggle, so a click
/// anywhere outside it closes the menu without any script. In the mobile
/// panel the toggle is a row naming the account, and the menu opens in place.
#[component]
fn AccountControls(
	user: SessionUser,
	sign_out: ServerAction<Logout>,
	menu: AccountMenu,
) -> impl IntoView {
	let location = use_location();
	let login = user.github_login.clone();

	view! {
		<ProjectsLink />
		<details class="group relative max-[1100px]:mt-3 max-[1100px]:border-t max-[1100px]:border-current/15 max-[1100px]:pt-3" node_ref=menu.menu>
			<summary class=ACCOUNT_TOGGLE node_ref=menu.toggle>
				<UserAvatar user=user />
				<span class="min-w-0 text-left min-[1101px]:sr-only">
					<span class="block text-[0.85rem] font-normal opacity-75"><span class="sr-only">"Account menu, "</span>"Signed in as"</span>
					<span class="block truncate">{login.clone()}</span>
				</span>
				<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16" aria-hidden="true" class="ml-auto shrink-0 transition-transform group-open:rotate-180"><path d="m6 9 6 6 6-6" /></svg>
			</summary>
			<div class="min-[1101px]:absolute min-[1101px]:right-0 min-[1101px]:top-[calc(100%+0.6rem)] min-[1101px]:z-10 min-[1101px]:w-72 min-[1101px]:rounded-2xl min-[1101px]:border min-[1101px]:border-rule min-[1101px]:bg-surface min-[1101px]:p-2 min-[1101px]:text-ink min-[1101px]:shadow-[0_24px_48px_-20px_rgb(25_21_46/0.35)] max-[1100px]:pl-11">
				<p class="mb-1 border-b border-rule px-3 pb-3 pt-2 text-[0.85rem] text-mute max-[1100px]:hidden">
					"Signed in as" <span class="block truncate text-base font-bold text-ink">{login}</span>
				</p>
				{ACCOUNT_LINKS.map(|(href, label)| view! {
					<a href=href target=move || cross_surface_target(&location.pathname.get(), href) class=MENU_ROW>{label}</a>
				})}
				<ActionForm action=sign_out>
					<button type="submit" class=MENU_ROW disabled=move || sign_out.pending().get()>"Sign out"</button>
				</ActionForm>
			</div>
		</details>
	}
}

#[cfg(test)]
#[path = "__tests__/navbar_tests.rs"]
mod tests;
