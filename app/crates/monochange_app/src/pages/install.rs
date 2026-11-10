//! CLI installation and the GitHub App workflow.

use leptoskit::prelude::*;

use crate::links::BOOK_URL;
use crate::ui::heading;
use crate::ui::link;
use crate::ui::text;

/// Install screen.
pub fn install_screen() -> Element {
	View()
		.class("flex flex-col gap-4")
		.child(heading(1, "Your first release plan starts here."))
		.child(text(
			"Install the CLI, point it at your workspace, and preview what comes next. It's free.",
		))
		.child(heading(2, "Install the CLI"))
		.child(text("The quickest route is the prebuilt npm package."))
		.child(text("npm install -g @monochange/cli\nmonochange --help"))
		.child(text("Prefer Cargo or Nix?"))
		.child(text(
			"cargo install monochange\n\n# Or run with Nix\nnix run github:ifiokjr/nixpkgs#monochange",
		))
		.child(heading(2, "Then make a plan."))
		.child(text(
			"Generate a starter configuration, discover your packages, and walk through your first changeset in the book.",
		))
		.child(link(BOOK_URL, "Open the monochange book"))
		.child(text("The first walkthrough stays local. Nothing is published."))
		.child(heading(2, "A GitHub App, when you want a bot."))
		.child(text(
			"The hosted workflow lets monochange create release commits and pull requests under its own bot identity.",
		))
		.child(text(
			"GitHub App installation is being finalized. The CLI is available now; hosted bot operations aren't available yet.",
		))
		.child(text(
			"The book explains repository access, GitHub Actions authentication, and the hosted release configuration for when the app is ready.",
		))
		.child(link(guide_href(), "Read the GitHub App setup guide"))
}

fn guide_href() -> String {
	#[cfg(not(target_arch = "wasm32"))]
	{
		crate::book::compiled_book()
			.ok()
			.and_then(|book| book.chapter("guide/github-app").cloned())
			.map(|chapter| chapter.href)
			.unwrap_or_else(|| format!("{BOOK_URL}/guide/github-app"))
	}
	#[cfg(target_arch = "wasm32")]
	{
		format!("{BOOK_URL}/guide/github-app")
	}
}

#[cfg(test)]
#[path = "__tests__/install_tests.rs"]
mod tests;
