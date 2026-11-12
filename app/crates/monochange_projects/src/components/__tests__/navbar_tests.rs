//! Navigation chrome coverage for home vs interior routes, and the signed-in
//! account controls both browser entries share.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;

fn user(avatar: Option<&str>) -> SessionUser {
	SessionUser {
		github_id: 101,
		github_login: "octo-cat".to_owned(),
		github_avatar_url: avatar.map(str::to_owned),
		plan_tier: "free".to_owned(),
	}
}

#[test]
fn escape_closes_mobile_navigation_and_the_account_menu() {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	runtime.block_on(async {
		Owner::new().with(|| {
			let (mobile_open, set_mobile_open) = signal(true);
			let account = AccountMenu::new();
			close_menus_on_key("Enter", set_mobile_open, account);
			assert!(mobile_open.get());
			close_menus_on_key("Escape", set_mobile_open, account);
			assert!(!mobile_open.get());
		});
	});
}

#[test]
fn sections_mark_the_current_page_and_every_book_chapter() {
	assert!(section_is_current("/install", "/install"));
	assert!(!section_is_current("/install/more", "/install"));
	assert!(section_is_current("/book", BOOK_URL));
	assert!(section_is_current("/book/guide/intro", BOOK_URL));
	assert!(!section_is_current("/pricing", BOOK_URL));
}

#[tokio::test]
async fn navbar_on_home_uses_hero_chrome() {
	let html = crate::tests::render_at("/", || view! { <NavBar /> }).await;
	assert!(html.contains("bg-hero text-mist"));
	assert!(html.contains("Skip to content"));
	assert!(html.contains("aria-label=\"Main navigation\""));
}

#[tokio::test]
async fn navbar_off_home_uses_page_chrome() {
	let html = crate::tests::render_at("/install", || view! { <NavBar /> }).await;
	assert!(html.contains("border-rule bg-page"));
	assert!(html.contains("aria-current=\"page\">Install</a>"), "{html}");
}

#[tokio::test]
async fn projects_navigation_leaves_the_product_entry_for_website_links() {
	let html = crate::tests::render_at("/projects", || view! { <NavBar /> }).await;
	assert!(
		html.contains("href=\"/install\" target=\"_self\""),
		"{html}"
	);
	assert!(html.contains("href=\"/book\" target=\"_self\""), "{html}");
}

#[tokio::test]
async fn the_account_menu_holds_every_account_action_behind_one_toggle() {
	let html = crate::tests::render_at("/projects/acme", || {
		let on_home = Memo::new(|_| false);
		view! { <AccountControls user=user(None) sign_out=ServerAction::new() on_home=on_home account=AccountMenu::new() /> }
	})
	.await;

	// Already in the workspace: the projects action is the current page.
	assert!(
		html.contains("href=\"/projects\" target=\"_self\" aria-current=\"page\""),
		"{html}"
	);
	assert!(html.contains("bg-brand-100 text-brand-800"), "{html}");
	assert!(html.contains("<details class=\"group relative\""), "{html}");
	assert!(
		html.contains("Account menu, signed in as <!>octo-cat"),
		"{html}"
	);
	assert!(
		html.contains("border-rule hover:border-brand-600"),
		"{html}"
	);
	// Account pages belong to the website entry: document navigations.
	assert!(
		html.contains("href=\"/dashboard\" target=\"_self\""),
		"{html}"
	);
	assert!(
		html.contains("href=\"/account\" target=\"_self\""),
		"{html}"
	);
	assert!(html.contains(">Account and devices</a>"), "{html}");
	assert!(html.contains(">Sign out</button>"), "{html}");
	// Without a GitHub avatar the toggle shows the login's initial.
	assert!(html.contains(">O</span>"), "{html}");
}

#[tokio::test]
async fn the_account_menu_on_the_website_keeps_navigation_client_side() {
	let html = crate::tests::render_at("/", || {
		let on_home = Memo::new(|_| true);
		view! { <AccountControls user=user(Some("https://avatars.example/octo.png")) sign_out=ServerAction::new() on_home=on_home account=AccountMenu::new() /> }
	})
	.await;

	assert!(!html.contains("aria-current"), "{html}");
	assert!(html.contains("bg-brand-700 text-white"), "{html}");
	assert!(
		html.contains("border-mist/25 hover:border-brand-400"),
		"{html}"
	);
	assert!(!html.contains("href=\"/dashboard\" target"), "{html}");
	assert!(
		html.contains("src=\"https://avatars.example/octo.png\""),
		"{html}"
	);
}

#[tokio::test]
async fn the_mobile_menu_lists_account_actions_as_rows() {
	let interior = crate::tests::render_at("/install", || {
		let on_home = Memo::new(|_| false);
		view! { <MobileAccount user=user(None) sign_out=ServerAction::new() on_home=on_home /> }
	})
	.await;
	assert!(interior.contains("border-t pt-3 border-rule"), "{interior}");
	assert!(interior.contains("text-ink\">octo-cat"), "{interior}");
	assert!(interior.contains(">Connect repositories</a>"), "{interior}");
	assert!(interior.contains("mt-4 w-full\""), "{interior}");

	let home = crate::tests::render_at("/", || {
		let on_home = Memo::new(|_| true);
		view! { <MobileAccount user=user(None) sign_out=ServerAction::new() on_home=on_home /> }
	})
	.await;
	assert!(home.contains("border-t pt-3 border-mist/15"), "{home}");
	assert!(home.contains("text-mist\">octo-cat"), "{home}");
	assert!(home.contains("hover:bg-mist/10"), "{home}");
}
