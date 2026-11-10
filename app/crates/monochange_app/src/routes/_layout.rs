//! Site chrome: a stack back control and the page.
//!
//! The tab bar is `navigator()`, which the file router shells around this layout.

use leptoskit::native::Role;
use leptoskit::prelude::*;
use leptoskit_router::Routable;

use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::ui::link;
use crate::ui::text;
use crate::ui::text_class;

/// Tab bar for the public site. Selecting a tab replaces history instead of
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

/// Wraps every screen. `navigation` is the file-router stack: Back pops it.
pub fn layout(children: Children, navigation: &crate::routes::Navigation) -> Element {
	let current = navigation.current();
	if project_shell(&current) {
		return project_layout(children);
	}
	let on_home = matches!(current, crate::routes::Route::Index);
	let skip = skip_link();
	let header = header(on_home);
	let back = back_control(navigation);
	let main = main_content(children);
	let footer = footer();

	view! {
		<View class="flex min-h-screen flex-col bg-white">
			{skip}
			{header}
			{back}
			{main}
			{footer}
		</View>
	}
}

/// Projects use their own shell. The marketing header is what carries
/// `aria-label="Main navigation"`, so it stays off these routes.
fn project_shell(route: &crate::routes::Route) -> bool {
	route.href().as_str().starts_with("/projects")
}

fn project_layout(children: Children) -> Element {
	let main = main_content(children);
	view! { <View class="flex min-h-screen flex-col bg-white">{main}</View> }
}

fn main_content(children: Children) -> Element {
	let mut collected = Vec::new();
	children.append_to(&mut collected);
	view! {
		<View role=Role::Main aria_label="Content" class="flex flex-col gap-6 p-6">
			{collected}
		</View>
	}
}

fn skip_link() -> Element {
	link("#main-content", "Skip to content")
}

fn back_control(navigation: &crate::routes::Navigation) -> Element {
	if !navigation.can_go_back() {
		return view! { <View /> };
	}

	let navigation = navigation.clone();
	view! {
		<Button
			class="rounded-lg px-4 py-2 font-semibold text-slate-900"
			on:press=move || {
				navigation.back();
			}
		>
			"Back"
		</Button>
	}
}

fn header(on_home: bool) -> Element {
	let brand = if on_home { "monochange" } else { "monochange" };
	let brand = link("/", brand);
	let docs = link(BOOK_URL, "Docs");
	let session = session_link();
	view! {
		<View
			role=Role::Banner
			class="flex flex-row flex-wrap items-center justify-between gap-4 border border-slate-200 p-4"
		>
			<View
				role=Role::Navigation
				aria_label="Main navigation"
				class="flex flex-row flex-wrap items-center gap-4"
			>
				{brand}
				{docs}
				{session}
			</View>
		</View>
	}
}

fn session_link() -> Element {
	use leptos::prelude::Get;

	match crate::slots::session().get() {
		Some(Ok(Some(_user))) => {
			let dashboard = link("/dashboard", "Dashboard");
			let sign_out = sign_out_button();
			view! {
				<View class="flex flex-row gap-4">
					{dashboard}
					{sign_out}
				</View>
			}
		}
		Some(Ok(None)) | None => link("/login", "Sign in"),
		Some(Err(_)) => link("/login", "Sign in again"),
	}
}

fn sign_out_button() -> Element {
	view! {
		<Button
			class="rounded-lg px-4 py-2 font-semibold text-slate-900"
			on:press=|| {
				leptos::task::spawn_local(async {
					let _ = crate::server_fns::auth::logout().await;
				});
			}
		>
			"Sign out"
		</Button>
	}
}

fn footer() -> Element {
	let tagline = text_class("text-slate-600", "Ship every package from one floor.");
	let install = link("/install", "Install");
	let book = link(BOOK_URL, "Read the book");
	let changelog = link("/changelog", "Changelog");
	let pricing = link("/pricing", "It's free");
	let source = link(SOURCE_URL, "GitHub");
	let author = link("https://github.com/ifiokjr", "Ifiok Jr.");
	let version = link(
		"/changelog",
		&format!("Website v{}", env!("CARGO_PKG_VERSION")),
	);
	let license = text("Free and open source.");
	view! {
		<View role=Role::ContentInfo class="flex flex-col gap-4 border border-slate-200 p-6">
			{tagline}
			<View
				role=Role::Navigation
				aria_label="Footer navigation"
				class="flex flex-row flex-wrap gap-4"
			>
				{install}
				{book}
				{changelog}
				{pricing}
				{source}
				{author}
				{version}
			</View>
			{license}
		</View>
	}
}
