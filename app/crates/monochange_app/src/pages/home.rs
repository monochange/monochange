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
	session.and_then(Result::ok).flatten().map_or_else(
		|| "/login?next=%2Fprojects".to_owned(),
		|_| "/projects".to_owned(),
	)
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

	let title = heading(1, "Ship every package from one floor.");
	let intro =
		text("Coordinate projects, releases, and feedback across the ecosystems you already ship.");
	let workspace = link(href, "Open your workspace");
	let floor = heading(2, "The product floor");
	let plan = heading(2, "One plan across packages");
	let ecosystems = heading(2, "Different ecosystems.");
	let same = text("Same plan.");
	let manifest = release_manifest(shared);
	view! {
		<View class="flex flex-col gap-6">
			{title}
			{intro}
			{workspace}
			{floor}
			{plan}
			{ecosystems}
			{same}
			{text("Cargo")}
			{text("Rust crates")}
			{text("npm")}
			{text("JavaScript & TypeScript")}
			{text("Dart")}
			{text("Dart & Flutter packages")}
			{text("Python")}
			{text("Python packages")}
			{text("Go")}
			{text("Go modules")}
			{text("Deno")}
			{text("Deno & JSR packages")}
			{heading(2, "Know what ships.")}
			{text("Before you ship it.")}
			{text("monochange preview")}
			{link(BOOK_URL, "Follow your first release plan")}
			{heading(3, "Describe the change")}
			{heading(3, "See the whole plan")}
			{heading(3, "Release on your terms")}
			{link("/install#github-app", "Explore the GitHub App workflow")}
			{image("/branding/mark.svg")}
			{heading(2, "Make the next release less complicated.")}
			{link("/install", "Get started for free")}
			{link(SOURCE_URL, "Explore the source")}
			{manifest}
		</View>
	}
}

fn release_manifest(shared: RwSignal<bool>) -> Element {
	let individual = shared;
	let shared_press = shared;
	view! {
		<View class="flex flex-col gap-4 rounded-xl border border-slate-200 p-6">
			{heading(2, "Your next release")}
			{text("Example workspace")}
			<View aria_label="Example version strategy" class="flex flex-row flex-wrap gap-2">
				<Button
					class="rounded-lg px-4 py-2 font-bold"
					on:press=move || individual.set(false)
				>
					"Individual versions"
				</Button>
				<Button
					class="rounded-lg bg-teal-700 px-4 py-2 font-bold text-white"
					on:press=move || shared_press.set(true)
				>
					"Shared version"
				</Button>
			</View>
			{text("@acme/ui")}
			{text("npm")}
			{text("1.4.2")}
			{text("1.5.0")}
			{text("acme-core")}
			{text("Cargo")}
			<Text>{move || core_versions(shared.get()).0.to_owned()}</Text>
			<Text>{move || core_versions(shared.get()).1.to_owned()}</Text>
			{text("acme-mobile")}
			{text("Dart")}
			<Text>{move || mobile_versions(shared.get()).0.to_owned()}</Text>
			<Text>{move || mobile_versions(shared.get()).1.to_owned()}</Text>
			<Text>{move || strategy_copy(shared.get()).to_owned()}</Text>
			{text("Preview first.")}
			{text("Publish when ready.")}
		</View>
	}
}

#[cfg(test)]
#[path = "__tests__/home_tests.rs"]
mod tests;
