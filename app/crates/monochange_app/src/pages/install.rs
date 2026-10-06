//! CLI installation and an honest introduction to the GitHub App workflow.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::links::BOOK_URL;

/// Give visitors a working install path and explain hosted app availability.
#[component]
pub fn InstallPage() -> impl IntoView {
	view! {
		<Title text="Install — monochange" />
		<section class="site-width install-section">
			<div class="page-intro"><h1>"Your first release plan" <br /> "starts here."</h1><p>"Install the CLI, point it at your workspace, and preview what comes next. It's free."</p></div>
			<div class="install-layout">
				<div class="install-command"><h2>"Install the CLI"</h2><p>"The quickest route is the prebuilt npm package."</p><pre><code>"npm install -g @monochange/cli\nmonochange --help"</code></pre><details><summary>"Prefer Cargo or Nix?"</summary><pre><code>"cargo install monochange\n\n# Or run with Nix\nnix run github:ifiokjr/nixpkgs#monochange"</code></pre></details></div>
				<div class="install-next"><h2>"Then make a plan."</h2><p>"Generate a starter configuration, discover your packages, and walk through your first changeset in the book."</p><a href=BOOK_URL class="button button-brand">"Open the monochange book" <ArrowIcon /></a><p class="small-note">"The first walkthrough stays local. Nothing is published."</p></div>
			</div>
			<section id="github-app" class="github-app-section"><div><h2>"A GitHub App," <br /> "when you want a bot."</h2><p>"The hosted workflow lets monochange create release commits and pull requests under its own bot identity."</p></div><div><p class="availability-note">"GitHub App installation is being finalized. The CLI is available now; hosted bot operations aren't available yet."</p><p>"The book explains repository access, GitHub Actions authentication, and the hosted release configuration for when the app is ready."</p><a href=format!("{BOOK_URL}guide/github-app.html") class="text-link">"Read the GitHub App setup guide" <ArrowIcon /></a></div></section>
		</section>
	}
}
