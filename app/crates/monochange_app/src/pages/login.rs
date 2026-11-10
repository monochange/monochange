//! GitHub sign-in and the OAuth callback.

use leptos::prelude::*;
use leptoskit::prelude::Element;
use leptoskit::prelude::Text;
use leptoskit::prelude::View;

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

	let mut column = View().class("flex flex-col gap-4 p-6");
	column = if for_portal {
		column
			.child(heading(1, "Your voice, counted once."))
			.child(text(
				"Sign in to share feedback, vote, and follow it until it ships. Your GitHub name isn't shown on the project's page.",
			))
	} else {
		column
			.child(heading(1, "Your workspace, all together."))
			.child(text(
				"Sign in to see your connected repositories and manage your monochange workspace.",
			))
			.child(link(BOOK_URL, "Just looking for the CLI? Read the book."))
	};

	column
		.child(image("/branding/mark.svg"))
		.child(heading(2, "Sign in to monochange"))
		.child(text("Use your GitHub account to continue."))
		.child(login_action(login_url))
		.child(text(
			"GitHub will show the account access requested before you continue.",
		))
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
			View()
				.class("flex flex-col gap-4")
				.child(heading(2, "Signed in!"))
				.child(Text(format!("Welcome, {}!", user.github_login)))
				.child(link("/dashboard", "Go to dashboard"))
		}
		Some(Err(error)) => {
			View()
				.class("flex flex-col gap-4")
				.child(heading(2, "Sign in failed"))
				.child(Text(error))
				.child(link("/login", "Try again"))
		}
	}
}
