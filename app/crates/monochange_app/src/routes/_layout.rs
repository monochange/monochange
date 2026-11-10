//! Site chrome: a stack back control, a tab row, and the page.

use leptoskit::native::Role;
use leptoskit::prelude::*;

use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::ui::link;
use crate::ui::text;
use crate::ui::text_class;

/// Tab bar for the public site. The file router on leptoskit `fa3d29f` wraps
/// this layout with the bar; selecting a tab replaces history instead of
/// pushing it.
pub fn navigator() -> leptoskit_router::navigators::Tabs<crate::routes::Route> {
	use leptoskit_router::navigators::Tab;
	use leptoskit_router::navigators::TabPlacement;
	use leptoskit_router::navigators::Tabs;

	Tabs::new([
		Tab::root(crate::routes::Route::Index).title("monochange"),
		Tab::root(crate::routes::Route::Install).title("Install"),
		Tab::root(crate::routes::Route::Changelog).title("Changelog"),
		Tab::root(crate::routes::Route::Pricing).title("Pricing"),
	])
	.placement(TabPlacement::Top)
}

/// Wraps every screen. `navigation` is the file-router stack: Back pops it,
/// and each tab link pushes the next screen.
pub fn layout(children: Children, navigation: &crate::routes::Navigation) -> Element {
	let current = navigation.current();
	let on_home = matches!(current, crate::routes::Route::Index);

	View()
		.class("flex min-h-screen flex-col bg-white")
		.child(skip_link())
		.child(header(on_home))
		.child(back_control(navigation))
		.child(main_content(children))
		.child(footer())
}

fn main_content(children: Children) -> Element {
	let mut collected = Vec::new();
	children.append_to(&mut collected);
	View()
		.role(Role::Main)
		.aria_label("Content")
		.class("flex flex-col gap-6 p-6")
		.children(collected)
}

fn skip_link() -> Element {
	link("#main-content", "Skip to content")
}

fn back_control(navigation: &crate::routes::Navigation) -> Element {
	if !navigation.can_go_back() {
		return View();
	}

	let navigation = navigation.clone();
	Button("Back")
		.class("rounded-lg px-4 py-2 font-semibold text-slate-900")
		.on_press(move || {
			navigation.back();
		})
}

fn header(on_home: bool) -> Element {
	let brand = if on_home { "monochange" } else { "monochange" };
	View()
		.role(Role::Banner)
		.class(
			"flex flex-row flex-wrap items-center justify-between gap-4 border border-slate-200 p-4",
		)
		.child(
			View()
				.role(Role::Navigation)
				.aria_label("Main navigation")
				.class("flex flex-row flex-wrap items-center gap-4")
				.child(link("/", brand))
				.child(link(BOOK_URL, "Docs"))
				.child(session_link()),
		)
}

fn session_link() -> Element {
	use leptos::prelude::Get;

	match crate::slots::session().get() {
		Some(Ok(Some(_user))) => {
			crate::ui::with_children(
				View().class("flex flex-row gap-4"),
				[link("/dashboard", "Dashboard"), sign_out_button()],
			)
		}
		Some(Ok(None)) | None => link("/login", "Sign in"),
		Some(Err(_)) => link("/login", "Sign in again"),
	}
}

fn sign_out_button() -> Element {
	Button("Sign out")
		.class("rounded-lg px-4 py-2 font-semibold text-slate-900")
		.on_press(|| {
			leptos::task::spawn_local(async {
				let _ = crate::server_fns::auth::logout().await;
			});
		})
}

fn footer() -> Element {
	View()
		.role(Role::ContentInfo)
		.class("flex flex-col gap-4 border border-slate-200 p-6")
		.child(text_class(
			"text-slate-600",
			"Ship every package from one floor.",
		))
		.child(
			View()
				.role(Role::Navigation)
				.aria_label("Footer navigation")
				.class("flex flex-row flex-wrap gap-4")
				.child(link("/install", "Install"))
				.child(link(BOOK_URL, "Read the book"))
				.child(link("/changelog", "Changelog"))
				.child(link("/pricing", "It's free"))
				.child(link(SOURCE_URL, "GitHub"))
				.child(link("https://github.com/ifiokjr", "Ifiok Jr."))
				.child(link(
					"/changelog",
					&format!("Website v{}", env!("CARGO_PKG_VERSION")),
				)),
		)
		.child(text("Free and open source."))
}
