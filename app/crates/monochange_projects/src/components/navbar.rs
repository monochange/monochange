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
/// One row in the account menu.
const MENU_ROW: &str = "flex min-h-11 w-full items-center rounded-[10px] px-3 text-left font-bold no-underline transition-colors hover:bg-page hover:text-brand-700 dark:hover:text-brand-300";

/// The site sections every page links to, in header order.
const SECTIONS: [(&str, &str); 4] = [
	("/install", "Install"),
	(BOOK_URL, "Docs"),
	("/changelog", "Changelog"),
	("/pricing", "Pricing"),
];

/// The account menu's disclosure and its toggle, so Escape and route changes
/// can close it and hand focus back.
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

/// Collapse the mobile menu and the account menu when Escape is pressed.
fn close_menus_on_key(key: &str, set_mobile_open: WriteSignal<bool>, account: AccountMenu) {
	if key == "Escape" {
		set_mobile_open.set(false);
		account.close(true);
	}
}

/// Public navigation, including a direct link to the published book.
#[component]
pub fn NavBar() -> impl IntoView {
	let _ = provide_color_mode();
	let color_mode = use_color_mode();
	let (mobile_open, set_mobile_open) = signal(false);
	let account = AccountMenu::new();
	let location = use_location();
	let sign_out = ServerAction::<Logout>::new();
	let session = Resource::new_blocking(
		move || (location.pathname.get(), sign_out.version().get()),
		|_| get_session(),
	);

	Effect::new(move || {
		location.pathname.get();
		set_mobile_open.set(false);
		// patch-coverage:ignore-start -- effects only run in the browser after hydration.
		account.close(false);
		// patch-coverage:ignore-end
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
				on:keydown=move |event| close_menus_on_key(&event.key(), set_mobile_open, account)
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
					{SECTIONS.map(|(href, label)| view! {
						<a href=href target=move || target(href) aria-current=move || section_is_current(&location.pathname.get(), href).then_some("page")>{label}</a>
					})}
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
						<SessionControls session=session sign_out=sign_out on_home=on_home account=account mobile=false />
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
					class=move || {
						if on_home.get() {
							"flex w-full flex-col gap-0.5 border-t border-mist/15 pb-5 pt-3 min-[1101px]:hidden [&>a]:flex [&>a]:min-h-11 [&>a]:items-center [&>a]:rounded-[10px] [&>a]:px-3 [&>a]:font-bold [&>a:hover]:bg-mist/10 [&>a[aria-current=page]]:text-brand-400"
						} else {
							"flex w-full flex-col gap-0.5 border-t border-rule pb-5 pt-3 min-[1101px]:hidden [&>a]:flex [&>a]:min-h-11 [&>a]:items-center [&>a]:rounded-[10px] [&>a]:px-3 [&>a]:font-bold [&>a:hover]:bg-surface [&>a[aria-current=page]]:text-brand-700 dark:[&>a[aria-current=page]]:text-brand-400"
						}
					}
					hidden=move || !mobile_open.get()
				>
					{SECTIONS.map(|(href, label)| view! {
						<a href=href target=move || target(href) aria-current=move || section_is_current(&location.pathname.get(), href).then_some("page")>{label}</a>
					})}
					<SessionControls session=session sign_out=sign_out on_home=on_home account=account mobile=true />
				</div>
			</nav>
		</header>
	}
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
#[component]
fn SessionControls(
	session: Resource<Result<Option<SessionUser>, server_fn::ServerFnError>>,
	sign_out: ServerAction<Logout>,
	on_home: Memo<bool>,
	account: AccountMenu,
	mobile: bool,
) -> impl IntoView {
	let location = use_location();
	let sign_in = move |label: &'static str| {
		let href = navigation_sign_in_path(&location.pathname.get());
		let target = cross_surface_target(&location.pathname.get(), &href);
		let class = if mobile {
			format!("{BUTTON} {BUTTON_BRAND} mt-4 w-full")
		} else {
			format!("{BUTTON} {BUTTON_BRAND}")
		};

		view! { <a href=href target=target class=class>{label}</a> }
	};

	view! {
		<Transition fallback=|| view! { <span class="sr-only" role="status">"Loading account…"</span> }>
			{move || session.get().map(|result| match result {
				Ok(Some(user)) if mobile => view! { <MobileAccount user=user sign_out=sign_out on_home=on_home /> }.into_any(),
				Ok(Some(user)) => view! { <AccountControls user=user sign_out=sign_out on_home=on_home account=account /> }.into_any(),
				Ok(None) => sign_in("Sign in").into_any(),
				Err(_) => sign_in("Sign in again").into_any(),
			})}

		</Transition>
	}
}

/// The workspace link: the header's primary action, tinted as the current
/// page while the visitor is already in the workspace.
#[component]
fn ProjectsLink(#[prop(optional)] full_width: bool) -> impl IntoView {
	let location = use_location();
	let current = move || is_projects_path(&location.pathname.get());
	let class = move || {
		let state = if current() {
			BUTTON_CURRENT
		} else {
			BUTTON_BRAND
		};
		let width = if full_width { "mt-4 w-full" } else { "" };

		format!("{BUTTON} {state} {width}")
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

/// Desktop: the projects action and a disclosure menu for the account. The
/// open menu draws a transparent full-viewport layer from its toggle, so a
/// click anywhere outside it closes the menu without any script.
#[component]
fn AccountControls(
	user: SessionUser,
	sign_out: ServerAction<Logout>,
	on_home: Memo<bool>,
	account: AccountMenu,
) -> impl IntoView {
	let location = use_location();
	let login = user.github_login.clone();
	let toggle = move || {
		format!(
			"flex min-h-11 list-none items-center gap-1.5 rounded-full border p-1 pr-2 transition-colors [&::-webkit-details-marker]:hidden group-open:before:fixed group-open:before:inset-0 group-open:before:cursor-default {}",
			if on_home.get() {
				"border-mist/25 hover:border-brand-400"
			} else {
				"border-rule hover:border-brand-600 dark:hover:border-brand-400"
			}
		)
	};

	view! {
		<ProjectsLink />
		<details class="group relative" node_ref=account.menu>
			<summary class=toggle node_ref=account.toggle>
				<UserAvatar user=user.clone() />
				<span class="sr-only">"Account menu, signed in as " {login.clone()}</span>
				<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16" aria-hidden="true" class="transition-transform group-open:rotate-180"><path d="m6 9 6 6 6-6" /></svg>
			</summary>
			<div class="absolute right-0 top-[calc(100%+0.6rem)] z-10 w-72 rounded-2xl border border-rule bg-surface p-2 text-ink shadow-[0_24px_48px_-20px_rgb(11_20_31/0.35)]">
				<p class="mb-1 border-b border-rule px-3 pb-3 pt-2 text-[0.85rem] text-mute">
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

/// Mobile: the same account actions as rows in the open menu, with the
/// projects action last as the menu's one filled button.
#[component]
fn MobileAccount(
	user: SessionUser,
	sign_out: ServerAction<Logout>,
	on_home: Memo<bool>,
) -> impl IntoView {
	let location = use_location();
	let login = user.github_login.clone();
	let palette = move || MobilePalette::for_page(on_home.get());
	let row = move || {
		format!(
			"flex min-h-11 w-full items-center rounded-[10px] px-3 text-left font-bold no-underline {}",
			palette().hover
		)
	};

	view! {
		<div class=move || format!("mt-3 border-t pt-3 {}", palette().rule)>
			<p class=move || format!("flex items-center gap-3 px-3 pb-2 text-[0.85rem] {}", palette().muted)>
				<UserAvatar user=user />
				<span class="min-w-0">"Signed in as" <span class=move || format!("block truncate text-base font-bold {}", palette().strong)>{login}</span></span>
			</p>
			{ACCOUNT_LINKS.map(|(href, label)| view! {
				<a href=href target=move || cross_surface_target(&location.pathname.get(), href) class=row>{label}</a>
			})}
			<ActionForm action=sign_out>
				<button type="submit" class=row disabled=move || sign_out.pending().get()>"Sign out"</button>
			</ActionForm>
			<ProjectsLink full_width=true />
		</div>
	}
}

/// Mobile menu colors: the homepage header sits on the dark hero in both
/// themes, every other header on the theme's page ground.
struct MobilePalette {
	rule: &'static str,
	muted: &'static str,
	strong: &'static str,
	hover: &'static str,
}

impl MobilePalette {
	fn for_page(on_home: bool) -> Self {
		if on_home {
			Self {
				rule: "border-mist/15",
				muted: "text-hero-muted",
				strong: "text-mist",
				hover: "hover:bg-mist/10",
			}
		} else {
			Self {
				rule: "border-rule",
				muted: "text-mute",
				strong: "text-ink",
				hover: "hover:bg-surface",
			}
		}
	}
}

#[cfg(test)]
#[path = "__tests__/navbar_tests.rs"]
mod tests;
