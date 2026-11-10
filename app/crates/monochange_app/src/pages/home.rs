//! Public introduction.

use leptoskit::prelude::*;

use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::server_fns::auth::SessionUser;
use crate::ui::heading;
use crate::ui::image;
use crate::ui::link;
use crate::ui::text;

/// Path for the hero call to action from the current session.
pub(crate) fn workspace_href_for(
	session: Option<Result<Option<SessionUser>, server_fn::ServerFnError>>,
) -> String {
	session
		.and_then(Result::ok)
		.flatten()
		.map_or_else(|| "/login".to_owned(), |_| "/dashboard".to_owned())
}

/// Current and next versions for the illustrative Cargo package row.
pub(crate) fn core_versions(shared: bool) -> (&'static str, &'static str) {
	if shared {
		("1.4.2", "1.5.0")
	} else {
		("0.8.1", "0.9.0")
	}
}

/// Current and next versions for the illustrative Dart package row.
pub(crate) fn mobile_versions(shared: bool) -> (&'static str, &'static str) {
	if shared {
		("1.4.2", "1.5.0")
	} else {
		("2.1.0", "2.1.1")
	}
}

/// Caption under the example release table.
pub(crate) fn strategy_copy(shared: bool) -> &'static str {
	if shared {
		"A version group gives these packages one shared release identity."
	} else {
		"Each package keeps its own version, in one coordinated plan."
	}
}

/// The marketing home screen.
pub fn home_screen() -> Element {
	let href = workspace_href_for(leptos::prelude::Get::get(&crate::slots::session()));
	let shared = RwSignal::new(false);

	View()
		.class("flex flex-col gap-6")
		.child(heading(1, "Ship every package from one floor."))
		.child(text(
			"Coordinate projects, releases, and feedback across the ecosystems you already ship.",
		))
		.child(link(href, "Open your workspace"))
		.child(heading(2, "The product floor"))
		.child(heading(2, "One plan across packages"))
		.child(heading(2, "Different ecosystems."))
		.child(text("Same plan."))
		.child(text("Cargo"))
		.child(text("Rust crates"))
		.child(text("npm"))
		.child(text("JavaScript & TypeScript"))
		.child(text("Dart"))
		.child(text("Dart & Flutter packages"))
		.child(text("Python"))
		.child(text("Python packages"))
		.child(text("Go"))
		.child(text("Go modules"))
		.child(text("Deno"))
		.child(text("Deno & JSR packages"))
		.child(heading(2, "Know what ships."))
		.child(text("Before you ship it."))
		.child(text("monochange preview"))
		.child(link(BOOK_URL, "Follow your first release plan"))
		.child(heading(3, "Describe the change"))
		.child(heading(3, "See the whole plan"))
		.child(heading(3, "Release on your terms"))
		.child(link(
			"/install#github-app",
			"Explore the GitHub App workflow",
		))
		.child(image("/branding/mark.svg"))
		.child(heading(2, "Make the next release less complicated."))
		.child(link("/install", "Get started for free"))
		.child(link(SOURCE_URL, "Explore the source"))
		.child(release_manifest(shared))
}

fn release_manifest(shared: RwSignal<bool>) -> Element {
	let individual = shared;
	let shared_press = shared;
	View()
		.class("flex flex-col gap-4 rounded-xl border border-slate-200 p-6")
		.child(heading(2, "Your next release"))
		.child(text("Example workspace"))
		.child(
			View()
				.aria_label("Example version strategy")
				.class("flex flex-row flex-wrap gap-2")
				.child(
					Button("Individual versions")
						.class("rounded-lg px-4 py-2 font-bold")
						.on_press(move || individual.set(false)),
				)
				.child(
					Button("Shared version")
						.class("rounded-lg bg-teal-700 px-4 py-2 font-bold text-white")
						.on_press(move || shared_press.set(true)),
				),
		)
		.child(text("@acme/ui"))
		.child(text("npm"))
		.child(text("1.4.2"))
		.child(text("1.5.0"))
		.child(text("acme-core"))
		.child(text("Cargo"))
		.child(Text(move || core_versions(shared.get()).0.to_owned()))
		.child(Text(move || core_versions(shared.get()).1.to_owned()))
		.child(text("acme-mobile"))
		.child(text("Dart"))
		.child(Text(move || mobile_versions(shared.get()).0.to_owned()))
		.child(Text(move || mobile_versions(shared.get()).1.to_owned()))
		.child(Text(move || strategy_copy(shared.get()).to_owned()))
		.child(text("Preview first."))
		.child(text("Publish when ready."))
}

#[cfg(test)]
#[path = "__tests__/home_tests.rs"]
mod tests;
