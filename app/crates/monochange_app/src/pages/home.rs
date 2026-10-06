//! Public introduction with an interactive, illustrative release manifest.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::components::brand_icon::BrandIcon;
use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;

/// Explain the release workflow before asking visitors to install it.
#[component]
pub fn HomePage() -> impl IntoView {
	view! {
		<Title text="monochange — Release planning for monorepos" />
		<section class="hero">
			<div class="site-width hero-layout">
				<div class="hero-copy">
					<h1>"Many packages." <br /> "One release plan."</h1>
					<p>"Changesets, versions, and release notes — together across your monorepo."</p>
					<div class="actions">
						<a href="/install" class="button button-light">"Install monochange" <ArrowIcon /></a>
						<a href=BOOK_URL class="button button-outline">"Read the book"</a>
					</div>
					<a href="/pricing" class="hero-note">"Free. Open source. Built with Rust."</a>
				</div>
				<ReleaseManifest />
			</div>
		</section>
		<section class="site-width ecosystem-section">
			<div class="section-intro">
				<h2>"Different ecosystems." <br /> "Same plan."</h2>
				<p>"Keep the tools your packages already use. monochange discovers your workspace, tracks dependencies, and brings the release decisions together."</p>
			</div>
			<ul class="ecosystem-list" aria-label="Supported package ecosystems">
				<li><strong><BrandIcon name="rust" />"Cargo"</strong><span>"Rust crates"</span></li>
				<li><strong><BrandIcon name="npm" />"npm"</strong><span>"JavaScript & TypeScript"</span></li>
				<li><strong><BrandIcon name="dart" />"Dart"</strong><span>"Dart & Flutter packages"</span></li>
				<li><strong><BrandIcon name="python" />"Python"</strong><span>"Python packages"</span></li>
				<li><strong><BrandIcon name="go" />"Go"</strong><span>"Go modules"</span></li>
				<li><strong><BrandIcon name="deno" />"Deno"</strong><span>"Deno & JSR packages"</span></li>
			</ul>
		</section>
		<section class="workflow-section">
			<div class="site-width workflow-layout">
				<div>
					<h2>"Know what ships." <br /> "Before you ship it."</h2>
					<p>"Start with a change. Review the versions and notes it produces. Publish when you're ready."</p>
					<div class="command-block"><code>"monochange preview"</code></div>
					<a href=BOOK_URL class="text-link">"Follow your first release plan" <ArrowIcon /></a>
				</div>
				<ol class="workflow-list">
					<li><h3>"Describe the change"</h3><p>"A changeset names the packages, the version bump, and the release note. Keep it beside the code it describes."</p></li>
					<li><h3>"See the whole plan"</h3><p>"Preview version changes, dependency updates, and changelogs together. Group packages when they share a release identity."</p></li>
					<li><h3>"Release on your terms"</h3><p>"Use the CLI locally or in CI. The GitHub App workflow adds a bot identity for release pull requests."</p><a href="/install#github-app">"Explore the GitHub App workflow"</a></li>
				</ol>
			</div>
		</section>
		<section class="site-width closing-section">
			<img src="/branding/mark.svg" width="72" height="72" alt="" />
			<h2>"Make your next release" <br /> "a little less complicated."</h2>
			<div class="actions">
				<a href="/install" class="button button-brand">"Get started for free" <ArrowIcon /></a>
				<a href=SOURCE_URL class="text-link">"Explore the source"</a>
			</div>
		</section>
	}
}

#[component]
fn ReleaseManifest() -> impl IntoView {
	let (shared, set_shared) = signal(false);

	view! {
		<div class="release-manifest">
			<div class="manifest-heading"><h2>"Your next release"</h2><p>"Example workspace"</p></div>
			<div class="manifest-controls" role="group" aria-label="Example version strategy">
				<button aria-pressed=move || (!shared.get()).to_string() on:click=move |_| set_shared.set(false)>"Individual versions"</button>
				<button aria-pressed=move || shared.get().to_string() on:click=move |_| set_shared.set(true)>"Shared version"</button>
			</div>
			<div class="manifest-table-wrap" aria-live="polite" aria-atomic="true">
				<table>
					<caption class="sr-only">"Illustrative release plan; choose individual or shared package versions above."</caption>
					<thead><tr><th scope="col">"Package"</th><th scope="col">"Current"</th><th scope="col">"Next"</th></tr></thead>
					<tbody>
						<tr><th scope="row"><strong>"@acme/ui"</strong><span><BrandIcon name="npm" />"npm"</span></th><td>"1.4.2"</td><td>"1.5.0"</td></tr>
						<tr><th scope="row"><strong>"acme-core"</strong><span><BrandIcon name="rust" />"Cargo"</span></th><td>{move || if shared.get() { "1.4.2" } else { "0.8.1" }}</td><td>{move || if shared.get() { "1.5.0" } else { "0.9.0" }}</td></tr>
						<tr><th scope="row"><strong>"acme-mobile"</strong><span><BrandIcon name="dart" />"Dart"</span></th><td>{move || if shared.get() { "1.4.2" } else { "2.1.0" }}</td><td>{move || if shared.get() { "1.5.0" } else { "2.1.1" }}</td></tr>
					</tbody>
				</table>
			</div>
			<p class="manifest-explanation">{move || if shared.get() { "A version group gives these packages one shared release identity." } else { "Each package keeps its own version, in one coordinated plan." }}</p>
			<div class="manifest-footer"><span>"Preview first."</span><span>"Publish when ready."</span></div>
		</div>
	}
}
