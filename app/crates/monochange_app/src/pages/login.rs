//! GitHub sign-in and the OAuth callback.

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptoskit::prelude::*;

use crate::links::BOOK_URL;
use crate::server_fns::auth::exchange_code;
use crate::server_fns::auth::get_login_url;
use crate::ui::heading;
use crate::ui::image;
use crate::ui::link;
use crate::ui::text;

/// Sign-in screen. A portal return (`next` starting with `/p/`) uses the
/// feedback copy.
pub fn login_screen(next: Option<String>) -> Element {
	let next_for_url = next.clone();
	let login_url = crate::slots::once(|| {
		Resource::new_blocking(
			move || next_for_url.clone(),
			|next| async move { get_login_url(next).await.unwrap_or_default() },
		)
	});
	let for_portal = next.as_deref().is_some_and(|next| next.starts_with("/p/"));

	let intro = if for_portal {
		leptoskit::view! {
			<View>
				{heading(1, "Your voice, counted once.")}
				{text(
					"Sign in to share feedback, vote, and follow it until it ships. Your GitHub name isn't shown on the project's page.",
				)}
			</View>
		}
	} else {
		leptoskit::view! {
			<View>
				{heading(1, "Your workspace, all together.")}
				{text(
					"Sign in to see your connected repositories and manage your monochange workspace.",
				)}
				{link(BOOK_URL, "Just looking for the CLI? Read the book.")}
			</View>
		}
	};
	let mark = image("/branding/mark.svg");
	let sign_in = heading(2, "Sign in to monochange");
	let continue_copy = text("Use your GitHub account to continue.");
	let action = login_action(login_url);
	let notice = text("GitHub will show the account access requested before you continue.");
	leptoskit::view! {
		<View class="flex flex-col gap-4 p-6">
			{intro}
			{mark}
			{sign_in}
			{continue_copy}
			{action}
			{notice}
		</View>
	}
}

fn login_action(login_url: Resource<String>) -> Element {
	match login_url.get() {
		None => text("Preparing GitHub sign-in..."),
		Some(url) if url.is_empty() => {
			text("GitHub sign-in is unavailable right now. Please try again later.")
		}
		Some(url) => link(url, "Continue with GitHub"),
	}
}

/// OAuth return. Exchanges the code during server rendering so the session
/// cookie is set before the HTML is sent.
pub fn callback_screen(code: Option<String>, state: Option<String>) -> Element {
	let code = code.unwrap_or_default();
	let state = state.unwrap_or_default();
	let result = crate::slots::once(|| {
		Resource::new_blocking(
			move || (code.clone(), state.clone()),
			|(code, state)| {
				async move {
					if code.is_empty() {
						return Err("No authorization code received".to_owned());
					}
					exchange_code(code, state)
						.await
						.map_err(|error| error.to_string())
				}
			},
		)
	});

	match result.get() {
		None => text("Completing sign in..."),
		Some(Ok(user)) => {
			let title = heading(2, "Signed in!");
			let welcome = format!("Welcome, {}!", user.github_login);
			let dashboard = link("/dashboard", "Go to dashboard");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{title}
					<Text>{welcome}</Text>
					{dashboard}
				</View>
			}
		}
		Some(Err(error)) => {
			let title = heading(2, "Sign in failed");
			let retry = link("/login", "Try again");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{title}
					<Text>{error}</Text>
					{retry}
				</View>
			}
		}
	}
}
