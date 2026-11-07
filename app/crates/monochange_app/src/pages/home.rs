//! Public introduction: Release floor hero, product surfaces, then demos.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::components::brand_icon::BrandIcon;
use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::server_fns::auth::get_session;

/// Explain the release workspace before asking visitors to install it.
#[component]
pub fn HomePage() -> impl IntoView {
	let session = Resource::new_blocking(|| (), |_| get_session());
	let workspace_href = Memo::new(move |_| {
		session
			.get()
			.and_then(Result::ok)
			.flatten()
			.map(|_| "/dashboard".to_owned())
			.unwrap_or_else(|| "/login".to_owned())
	});

	view! {
		<Title text="monochange — Release planning for monorepos" />
		<HomeHero workspace_href=workspace_href />
		<ProductFloor workspace_href=workspace_href />
		<ReleasePlanDemo />
		<EcosystemSection />
		<WorkflowSection />
		<ClosingSection />
	}
}

/// Full-bleed brand-first hero. No cards or demos in the first viewport.
#[component]
fn HomeHero(workspace_href: Memo<String>) -> impl IntoView {
	view! {
		<section class="hero" aria-label="monochange">
			<div class="hero-atmosphere" aria-hidden="true">
				<div class="hero-grid"></div>
				<div class="hero-lane"></div>
			</div>
			<div class="site-width hero-layout">
				<div class="hero-brand">
					<img src="/branding/mark.svg" width="72" height="72" alt="" />
					<img
						src="/branding/wordmark-dark.svg"
						width="320"
						height="60"
						alt="monochange"
						class="brand-wordmark"
					/>
				</div>
				<div class="hero-copy">
					<h1>"Ship every package from one floor."</h1>
					<p>
						"A workspace for projects, releases, feedback, and the org that owns them."
					</p>
					<div class="actions">
						<a href=move || workspace_href.get() class="button button-light">
							"Open workspace"
							<ArrowIcon />
						</a>
						<a href="/install" class="button button-outline">
							"Install CLI"
						</a>
					</div>
					<a href="/pricing" class="hero-note">
						"Free. Open source."
					</a>
				</div>
			</div>
		</section>
	}
}

/// Linked project-management surfaces under the hero.
#[component]
fn ProductFloor(workspace_href: Memo<String>) -> impl IntoView {
	view! {
		<section class="site-width product-floor" aria-labelledby="product-floor-heading">
			<div class="product-floor-intro">
				<h2 id="product-floor-heading">"The product floor"</h2>
				<p>
					"monochange is a project-management surface for shipping many packages — not only a CLI."
				</p>
			</div>
			<nav class="product-floor-list" aria-label="Product surfaces">
				<a href=move || workspace_href.get()>
					<span class="floor-index" aria-hidden="true">"01"</span>
					<strong>"Projects"</strong>
					<span>"Group repos under one release identity."</span>
					<span class="floor-arrow" aria-hidden="true">"→"</span>
				</a>
				<a href=BOOK_URL>
					<span class="floor-index" aria-hidden="true">"02"</span>
					<strong>"Releases"</strong>
					<span>"Preview versions and notes before you publish."</span>
					<span class="floor-arrow" aria-hidden="true">"→"</span>
				</a>
				<a href=move || workspace_href.get()>
					<span class="floor-index" aria-hidden="true">"03"</span>
					<strong>"Feedback"</strong>
					<span>"Contributor portal and maintainer console in one place."</span>
					<span class="floor-arrow" aria-hidden="true">"→"</span>
				</a>
				<a href=move || workspace_href.get()>
					<span class="floor-index" aria-hidden="true">"04"</span>
					<strong>"Org / dashboard"</strong>
					<span>"Connected orgs, installs, and permissions."</span>
					<span class="floor-arrow" aria-hidden="true">"→"</span>
				</a>
			</nav>
		</section>
	}
}

/// Interactive release-plan demo (below the fold).
#[component]
fn ReleasePlanDemo() -> impl IntoView {
	view! {
		<section class="site-width demo-section" aria-labelledby="demo-heading">
			<div class="demo-intro">
				<h2 id="demo-heading">"One plan across packages"</h2>
				<p>
					"Try an example workspace: keep individual versions, or share one release identity."
				</p>
			</div>
			<ReleaseManifest />
		</section>
	}
}

#[component]
fn EcosystemSection() -> impl IntoView {
	view! {
		<section class="site-width ecosystem-section">
			<div class="section-intro">
				<h2>"Different ecosystems." <br /> "Same plan."</h2>
				<p>
					"Keep the tools your packages already use. monochange discovers your workspace, tracks dependencies, and brings the release decisions together."
				</p>
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
	}
}

#[component]
fn WorkflowSection() -> impl IntoView {
	view! {
		<section class="workflow-section">
			<div class="site-width workflow-layout">
				<div>
					<h2>"Know what ships." <br /> "Before you ship it."</h2>
					<p>
						"Start with a change. Review the versions and notes it produces. Publish when you're ready."
					</p>
					<div class="command-block"><code>"monochange preview"</code></div>
					<a href=BOOK_URL class="text-link">
						"Follow your first release plan"
						<ArrowIcon />
					</a>
				</div>
				<ol class="workflow-list">
					<li>
						<h3>"Describe the change"</h3>
						<p>
							"A changeset names the packages, the version bump, and the release note. Keep it beside the code it describes."
						</p>
					</li>
					<li>
						<h3>"See the whole plan"</h3>
						<p>
							"Preview version changes, dependency updates, and changelogs together. Group packages when they share a release identity."
						</p>
					</li>
					<li>
						<h3>"Release on your terms"</h3>
						<p>
							"Use the CLI locally or in CI. The GitHub App workflow adds a bot identity for release pull requests."
						</p>
						<a href="/install#github-app">"Explore the GitHub App workflow"</a>
					</li>
				</ol>
			</div>
		</section>
	}
}

#[component]
fn ClosingSection() -> impl IntoView {
	view! {
		<section class="site-width closing-section">
			<img src="/branding/mark.svg" width="72" height="72" alt="" />
			<h2>"Make the next release less complicated."</h2>
			<div class="actions">
				<a href="/install" class="button button-brand">
					"Get started for free"
					<ArrowIcon />
				</a>
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
			<div class="manifest-heading">
				<h2>"Your next release"</h2>
				<p>"Example workspace"</p>
			</div>
			<div class="manifest-controls" role="group" aria-label="Example version strategy">
				<button
					aria-pressed=move || (!shared.get()).to_string()
					on:click=move |_| set_shared.set(false)
				>
					"Individual versions"
				</button>
				<button
					aria-pressed=move || shared.get().to_string()
					on:click=move |_| set_shared.set(true)
				>
					"Shared version"
				</button>
			</div>
			<div class="manifest-table-wrap" aria-live="polite" aria-atomic="true">
				<table>
					<caption class="sr-only">
						"Illustrative release plan; choose individual or shared package versions above."
					</caption>
					<thead>
						<tr>
							<th scope="col">"Package"</th>
							<th scope="col">"Current"</th>
							<th scope="col">"Next"</th>
						</tr>
					</thead>
					<tbody>
						<tr>
							<th scope="row">
								<strong>"@acme/ui"</strong>
								<span><BrandIcon name="npm" />"npm"</span>
							</th>
							<td>"1.4.2"</td>
							<td>"1.5.0"</td>
						</tr>
						<tr>
							<th scope="row">
								<strong>"acme-core"</strong>
								<span><BrandIcon name="rust" />"Cargo"</span>
							</th>
							<td>{move || if shared.get() { "1.4.2" } else { "0.8.1" }}</td>
							<td>{move || if shared.get() { "1.5.0" } else { "0.9.0" }}</td>
						</tr>
						<tr>
							<th scope="row">
								<strong>"acme-mobile"</strong>
								<span><BrandIcon name="dart" />"Dart"</span>
							</th>
							<td>{move || if shared.get() { "1.4.2" } else { "2.1.0" }}</td>
							<td>{move || if shared.get() { "1.5.0" } else { "2.1.1" }}</td>
						</tr>
					</tbody>
				</table>
			</div>
			<p class="manifest-explanation">
				{move || {
					if shared.get() {
						"A version group gives these packages one shared release identity."
					} else {
						"Each package keeps its own version, in one coordinated plan."
					}
				}}
			</p>
			<div class="manifest-footer">
				<span>"Preview first."</span>
				<span>"Publish when ready."</span>
			</div>
		</div>
	}
}
