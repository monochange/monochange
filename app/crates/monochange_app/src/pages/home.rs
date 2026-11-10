//! Public introduction: Release floor hero, product surfaces, then demos.
//!
//! Landing markup uses Tailwind utilities mapped to `@theme` / `--mc-*` tokens.
//! Do not add semantic CSS class stacks for these surfaces.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::components::brand_icon::BrandIcon;
use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::server_fns::auth::SessionUser;
use crate::server_fns::auth::get_session;

const SITE: &str = "mx-auto w-[min(80rem,calc(100%-6rem))] max-[1050px]:w-[calc(100%-4rem)] max-[760px]:w-[calc(100%-2.5rem)]";
const BTN: &str = "inline-flex min-h-12 items-center justify-center gap-4 rounded-xl px-[1.35rem] py-[0.85rem] font-bold leading-snug no-underline transition-colors";
const BTN_BRAND: &str = "bg-brand-700 text-white hover:bg-brand-800";
const BTN_LIGHT: &str = "bg-brand-100 text-brand-700 hover:bg-brand-200";
const BTN_OUTLINE: &str = "border border-brand-300 text-mist hover:bg-brand-600/20";
const TEXT_LINK: &str =
	"inline-block py-[0.65rem] font-bold text-brand-600 underline dark:text-brand-400";
const DISPLAY: &str = "font-display text-[clamp(2.4rem,3.2vw,3.4rem)] font-extrabold tracking-[-0.035em] leading-[1.06]";
const FLOOR_ROW: &str = "group grid min-h-12 grid-cols-[2.5rem_minmax(0,14rem)_minmax(0,1fr)_auto] items-baseline gap-x-6 gap-y-1 border-b border-rule py-[1.45rem] text-inherit no-underline transition-[color,padding] hover:pl-1.5 hover:text-brand-600 max-[760px]:grid-cols-[2.5rem_minmax(0,1fr)_auto] max-[760px]:gap-x-3.5 dark:hover:text-brand-400";

/// Path for the hero/workspace CTA from the current session resource value.
fn workspace_href_for(
	session: Option<Result<Option<SessionUser>, server_fn::ServerFnError>>,
) -> String {
	session.and_then(Result::ok).flatten().map_or_else(
		|| "/login?next=%2Fprojects".to_owned(),
		|_| "/projects".to_owned(),
	)
}

/// Explain the release workspace before asking visitors to install it.
#[component]
pub fn HomePage() -> impl IntoView {
	let session = Resource::new_blocking(|| (), |()| get_session());
	let workspace_href = Memo::new(move |_| workspace_href_for(session.get()));

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
		<section
			class="relative flex min-h-[min(100svh,56rem)] items-end overflow-hidden bg-hero text-mist max-[760px]:min-h-[min(100svh,44rem)]"
			aria-label="monochange"
		>
			<div
				class="pointer-events-none absolute -left-[8%] -top-[18%] h-[42rem] w-[42rem] rounded-full bg-[radial-gradient(circle,var(--mc-glow),transparent_70%)] opacity-90 blur-md animate-[hero-breathe_12s_ease-in-out_infinite]"
				aria-hidden="true"
			></div>
			<div
				class="pointer-events-none absolute -bottom-[20%] -right-[10%] h-[34rem] w-[34rem] rounded-full bg-[radial-gradient(circle,var(--mc-glow),transparent_70%)] opacity-70 blur-md animate-[hero-breathe_12s_ease-in-out_infinite] [animation-delay:-6s]"
				aria-hidden="true"
			></div>
			<div class="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
				<div class="absolute inset-x-0 -top-[10%] bottom-[20%] origin-top scale-[1.45] opacity-[0.42] [background-image:linear-gradient(color-mix(in_srgb,var(--mc-hero-fg)_10%,transparent)_1px,transparent_1px),linear-gradient(90deg,color-mix(in_srgb,var(--mc-hero-fg)_10%,transparent)_1px,transparent_1px)] [background-size:72px_72px] [mask-image:linear-gradient(180deg,transparent_0%,#000_28%,#000_62%,transparent_100%)] [transform:perspective(900px)_rotateX(62deg)]"></div>
				<div class="absolute bottom-0 left-[8%] right-[8%] h-px bg-gradient-to-r from-transparent via-signal to-transparent opacity-55"></div>
			</div>
			<div class=format!(
				"{SITE} relative z-10 grid w-full grid-cols-1 items-end gap-11 py-[clamp(6rem,14vh,9rem)_clamp(4rem,8vh,5.5rem)] max-[1050px]:gap-8 max-[1050px]:py-16 max-[760px]:gap-6 max-[760px]:py-20 max-[760px]:pb-14"
			)>
				<div class="flex animate-[brand-rise_0.9s_cubic-bezier(0.16,1,0.3,1)_both] items-center gap-[1.15rem]">
					<img src="/branding/mark.svg" width="72" height="72" alt="" class="size-[72px] max-[760px]:size-14" />
					<img
						src="/branding/wordmark-dark.svg"
						width="320"
						height="60"
						alt="monochange"
						class="h-auto w-[min(320px,72vw)] max-[760px]:w-[min(240px,68vw)]"
					/>
				</div>
				<div class="max-w-xl animate-[brand-rise_0.9s_cubic-bezier(0.16,1,0.3,1)_0.08s_both]">
					<h1 class="font-display max-w-[15ch] text-[clamp(3.1rem,4.6vw,4.75rem)] font-extrabold tracking-[-0.035em] leading-[1.06] max-[1050px]:text-[3.3rem] max-[760px]:text-[clamp(2.4rem,9vw,3.6rem)]">
						"Ship every package from one floor."
					</h1>
					<p class="mt-6 max-w-[42ch] text-[1.3rem] text-hero-muted max-[760px]:mt-4 max-[760px]:text-[1.05rem]">
						"A workspace for projects, releases, feedback, and the org that owns them."
					</p>
					<div class="mt-8 flex flex-wrap items-center gap-4 max-[760px]:mt-5 max-[760px]:gap-2.5">
						<a href=move || workspace_href.get() class=format!("{BTN} {BTN_LIGHT}")>
							"Open workspace"
							<ArrowIcon />
						</a>
						<a href="/install" class=format!("{BTN} {BTN_OUTLINE}")>
							"Install CLI"
						</a>
					</div>
					<a href="/pricing" class="mt-6 inline-block text-hero-muted underline max-[760px]:mt-4 max-[760px]:text-[0.9rem]">
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
		<section class=format!("{SITE} py-[6.5rem] max-[760px]:py-16") aria-labelledby="product-floor-heading">
			<div class="mb-10 grid max-w-3xl gap-4">
				<h2 id="product-floor-heading" class=DISPLAY>"The product floor"</h2>
				<p class="max-w-[50ch] text-[1.15rem] text-mute">
					"monochange is a project-management surface for shipping many packages — not only a CLI."
				</p>
			</div>
			<nav class="grid border-t border-rule" aria-label="Product surfaces">
				<a href=move || workspace_href.get() class=FLOOR_ROW>
					<span class="font-mono text-[0.78rem] font-bold tracking-wider text-brand-600 dark:text-brand-400" aria-hidden="true">"01"</span>
					<strong class="text-[1.35rem] font-bold">"Projects"</strong>
					<span class="max-w-[48ch] text-[1.05rem] text-mute max-[760px]:text-[0.95rem]">"Group repos under one release identity."</span>
					<span class="font-bold text-brand-600 transition-transform group-hover:translate-x-1 dark:text-brand-400" aria-hidden="true">"→"</span>
				</a>
				<a href=BOOK_URL class=FLOOR_ROW>
					<span class="font-mono text-[0.78rem] font-bold tracking-wider text-brand-600 dark:text-brand-400" aria-hidden="true">"02"</span>
					<strong class="text-[1.35rem] font-bold">"Releases"</strong>
					<span class="max-w-[48ch] text-[1.05rem] text-mute max-[760px]:text-[0.95rem]">"Preview versions and notes before you publish."</span>
					<span class="font-bold text-brand-600 transition-transform group-hover:translate-x-1 dark:text-brand-400" aria-hidden="true">"→"</span>
				</a>
				<a href=move || workspace_href.get() class=FLOOR_ROW>
					<span class="font-mono text-[0.78rem] font-bold tracking-wider text-brand-600 dark:text-brand-400" aria-hidden="true">"03"</span>
					<strong class="text-[1.35rem] font-bold">"Feedback"</strong>
					<span class="max-w-[48ch] text-[1.05rem] text-mute max-[760px]:text-[0.95rem]">"Contributor portal and maintainer console in one place."</span>
					<span class="font-bold text-brand-600 transition-transform group-hover:translate-x-1 dark:text-brand-400" aria-hidden="true">"→"</span>
				</a>
				<a href=move || workspace_href.get() class=FLOOR_ROW>
					<span class="font-mono text-[0.78rem] font-bold tracking-wider text-brand-600 dark:text-brand-400" aria-hidden="true">"04"</span>
					<strong class="text-[1.35rem] font-bold">"Org / dashboard"</strong>
					<span class="max-w-[48ch] text-[1.05rem] text-mute max-[760px]:text-[0.95rem]">"Connected orgs, installs, and permissions."</span>
					<span class="font-bold text-brand-600 transition-transform group-hover:translate-x-1 dark:text-brand-400" aria-hidden="true">"→"</span>
				</a>
			</nav>
		</section>
	}
}

/// Interactive release-plan demo (below the fold).
#[component]
fn ReleasePlanDemo() -> impl IntoView {
	view! {
		<section class=format!("{SITE} pb-[var(--mc-space-section)]") aria-labelledby="demo-heading">
			<div class="mb-8 grid max-w-3xl gap-4">
				<h2 id="demo-heading" class=DISPLAY>"One plan across packages"</h2>
				<p class="max-w-[50ch] text-[1.1rem] text-mute">
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
		<section class=format!("{SITE} py-[6.5rem] max-[760px]:py-16")>
			<div class="grid grid-cols-2 items-end gap-16 max-[760px]:grid-cols-1 max-[760px]:gap-6">
				<h2 class=DISPLAY>"Different ecosystems." <br /> "Same plan."</h2>
				<p class="max-w-[50ch] text-[1.15rem] text-mute">
					"Keep the tools your packages already use. monochange discovers your workspace, tracks dependencies, and brings the release decisions together."
				</p>
			</div>
			<ul class="mt-12 grid grid-cols-3 gap-x-12 max-[1050px]:grid-cols-2 max-[760px]:gap-x-6" aria-label="Supported package ecosystems">
				<li class="flex items-center justify-between gap-4 border-t border-rule py-[1.1rem] max-[760px]:block">
					<strong class="inline-flex items-center gap-2.5 text-[1.35rem]"><BrandIcon name="rust" />"Cargo"</strong>
					<span class="text-[0.95rem] text-mute max-[760px]:mt-0 max-[760px]:block max-[760px]:text-left max-[760px]:text-[0.85rem]">"Rust crates"</span>
				</li>
				<li class="flex items-center justify-between gap-4 border-t border-rule py-[1.1rem] max-[760px]:block">
					<strong class="inline-flex items-center gap-2.5 text-[1.35rem]"><BrandIcon name="npm" />"npm"</strong>
					<span class="text-[0.95rem] text-mute max-[760px]:block max-[760px]:text-[0.85rem]">"JavaScript & TypeScript"</span>
				</li>
				<li class="flex items-center justify-between gap-4 border-t border-rule py-[1.1rem] max-[760px]:block">
					<strong class="inline-flex items-center gap-2.5 text-[1.35rem]"><BrandIcon name="dart" />"Dart"</strong>
					<span class="text-[0.95rem] text-mute max-[760px]:block max-[760px]:text-[0.85rem]">"Dart & Flutter packages"</span>
				</li>
				<li class="flex items-center justify-between gap-4 border-t border-rule py-[1.1rem] max-[760px]:block">
					<strong class="inline-flex items-center gap-2.5 text-[1.35rem]"><BrandIcon name="python" />"Python"</strong>
					<span class="text-[0.95rem] text-mute max-[760px]:block max-[760px]:text-[0.85rem]">"Python packages"</span>
				</li>
				<li class="flex items-center justify-between gap-4 border-t border-rule py-[1.1rem] max-[760px]:block">
					<strong class="inline-flex items-center gap-2.5 text-[1.35rem]"><BrandIcon name="go" />"Go"</strong>
					<span class="text-[0.95rem] text-mute max-[760px]:block max-[760px]:text-[0.85rem]">"Go modules"</span>
				</li>
				<li class="flex items-center justify-between gap-4 border-t border-rule py-[1.1rem] max-[760px]:block">
					<strong class="inline-flex items-center gap-2.5 text-[1.35rem]"><BrandIcon name="deno" />"Deno"</strong>
					<span class="text-[0.95rem] text-mute max-[760px]:block max-[760px]:text-[0.85rem]">"Deno & JSR packages"</span>
				</li>
			</ul>
		</section>
	}
}

#[component]
fn WorkflowSection() -> impl IntoView {
	view! {
		<section class="bg-ink py-[6.5rem] text-mist max-[760px]:py-16">
			<div class=format!("{SITE} grid grid-cols-2 gap-24 max-[1050px]:gap-12 max-[760px]:grid-cols-1")>
				<div>
					<h2 class=DISPLAY>"Know what ships." <br /> "Before you ship it."</h2>
					<p class="mt-6 max-w-[50ch] text-hero-muted">
						"Start with a change. Review the versions and notes it produces. Publish when you're ready."
					</p>
					<div class="my-8 overflow-x-auto whitespace-pre rounded-xl border border-rule p-6 font-mono text-[0.9rem]">
						<code>"monochange preview"</code>
					</div>
					<a href=BOOK_URL class=TEXT_LINK>
						"Follow your first release plan"
						<ArrowIcon />
					</a>
				</div>
				<ol class="list-none [counter-reset:step]">
					<li class="relative mb-8 border-b border-mist/20 pb-8 pl-12 [counter-increment:step] before:absolute before:left-0 before:top-0 before:font-bold before:text-brand-400 before:content-[counter(step)] last:mb-0 last:border-0 last:pb-0">
						<h3 class="text-[1.4rem] font-bold">"Describe the change"</h3>
						<p class="mt-2.5 text-hero-muted">
							"A changeset names the packages, the version bump, and the release note. Keep it beside the code it describes."
						</p>
					</li>
					<li class="relative mb-8 border-b border-mist/20 pb-8 pl-12 [counter-increment:step] before:absolute before:left-0 before:top-0 before:font-bold before:text-brand-400 before:content-[counter(step)] last:mb-0 last:border-0 last:pb-0">
						<h3 class="text-[1.4rem] font-bold">"See the whole plan"</h3>
						<p class="mt-2.5 text-hero-muted">
							"Preview version changes, dependency updates, and changelogs together. Group packages when they share a release identity."
						</p>
					</li>
					<li class="relative mb-8 border-b border-mist/20 pb-8 pl-12 [counter-increment:step] before:absolute before:left-0 before:top-0 before:font-bold before:text-brand-400 before:content-[counter(step)] last:mb-0 last:border-0 last:pb-0">
						<h3 class="text-[1.4rem] font-bold">"Release on your terms"</h3>
						<p class="mt-2.5 text-hero-muted">
							"Use the CLI locally or in CI. The GitHub App workflow adds a bot identity for release pull requests."
						</p>
						<a href="/install#github-app" class="inline-block pt-2.5 text-brand-400 underline">"Explore the GitHub App workflow"</a>
					</li>
				</ol>
			</div>
		</section>
	}
}

#[component]
fn ClosingSection() -> impl IntoView {
	view! {
		<section class=format!("{SITE} py-24 text-center max-[760px]:py-16")>
			<img src="/branding/mark.svg" width="72" height="72" alt="" class="mx-auto mb-8" />
			<h2 class=DISPLAY>"Make the next release less complicated."</h2>
			<div class="mt-8 flex flex-wrap items-center justify-center gap-4">
				<a href="/install" class=format!("{BTN} {BTN_BRAND}")>
					"Get started for free"
					<ArrowIcon />
				</a>
				<a href=SOURCE_URL class=TEXT_LINK>"Explore the source"</a>
			</div>
		</section>
	}
}

/// Tailwind classes for the example version-strategy toggle.
fn strategy_button_class(pressed: bool) -> String {
	format!(
		"min-h-11 rounded-lg px-3.5 py-2.5 text-[0.9rem] font-bold {} {}",
		if pressed {
			"bg-brand-100 text-brand-700 dark:bg-brand-900 dark:text-brand-200"
		} else {
			"text-mute hover:bg-page"
		},
		""
	)
}

/// Current/next versions for the illustrative Cargo package row.
fn core_versions(shared: bool) -> (&'static str, &'static str) {
	if shared {
		("1.4.2", "1.5.0")
	} else {
		("0.8.1", "0.9.0")
	}
}

/// Current/next versions for the illustrative Dart package row.
fn mobile_versions(shared: bool) -> (&'static str, &'static str) {
	if shared {
		("1.4.2", "1.5.0")
	} else {
		("2.1.0", "2.1.1")
	}
}

/// Caption under the example release table.
fn strategy_copy(shared: bool) -> &'static str {
	if shared {
		"A version group gives these packages one shared release identity."
	} else {
		"Each package keeps its own version, in one coordinated plan."
	}
}

fn select_individual_versions(set_shared: WriteSignal<bool>) {
	set_shared.set(false);
}

fn select_shared_version(set_shared: WriteSignal<bool>) {
	set_shared.set(true);
}

#[component]
fn ReleaseManifest() -> impl IntoView {
	let (shared, set_shared) = signal(false);

	view! {
		<div class="animate-[manifest-arrive_0.7s_cubic-bezier(0.16,1,0.3,1)] rounded-2xl border border-rule bg-surface p-8 text-ink shadow-[0_24px_48px_color-mix(in_srgb,var(--mc-ink)_12%,transparent)] max-[1050px]:p-[1.4rem]">
			<div class="flex items-start justify-between gap-4 max-[1050px]:flex-col max-[1050px]:gap-1.5 max-[760px]:flex-row">
				<h2 class="font-display text-[1.9rem] font-extrabold tracking-[-0.035em] max-[760px]:text-[1.6rem]">"Your next release"</h2>
				<p class="max-w-[12ch] text-right text-[0.85rem] text-mute max-[1050px]:max-w-none max-[760px]:max-w-[12ch]">"Example workspace"</p>
			</div>
			<div class="my-6 flex flex-wrap gap-1.5 max-[760px]:my-4" role="group" aria-label="Example version strategy">
				<button
					class=move || strategy_button_class(!shared.get())
					aria-pressed=move || (!shared.get()).to_string()
					on:click=move |_| select_individual_versions(set_shared)
				>
					"Individual versions"
				</button>
				<button
					class=move || strategy_button_class(shared.get())
					aria-pressed=move || shared.get().to_string()
					on:click=move |_| select_shared_version(set_shared)
				>
					"Shared version"
				</button>
			</div>
			<div class="overflow-x-auto" aria-live="polite" aria-atomic="true">
				<table class="w-full border-collapse text-left tabular-nums">
					<caption class="sr-only">
						"Illustrative release plan; choose individual or shared package versions above."
					</caption>
					<thead>
						<tr>
							<th scope="col" class="py-2 text-[0.75rem] font-bold text-mute">"Package"</th>
							<th scope="col" class="py-2 text-[0.75rem] font-bold text-mute">"Current"</th>
							<th scope="col" class="py-2 text-[0.75rem] font-bold text-mute">"Next"</th>
						</tr>
					</thead>
					<tbody>
						<tr class="border-b border-rule">
							<th scope="row" class="py-[1.15rem] max-[760px]:py-[0.85rem]">
								<strong class="block text-[1.1rem] max-[760px]:text-[0.95rem]">"@acme/ui"</strong>
								<span class="block text-[0.8rem] font-normal text-mute"><BrandIcon name="npm" />"npm"</span>
							</th>
							<td class="whitespace-nowrap text-[0.95rem] max-[760px]:text-[0.85rem]">"1.4.2"</td>
							<td class="whitespace-nowrap text-[0.95rem] font-bold text-brand-700 max-[760px]:text-[0.85rem]">"1.5.0"</td>
						</tr>
						<tr class="border-b border-rule">
							<th scope="row" class="py-[1.15rem] max-[760px]:py-[0.85rem]">
								<strong class="block text-[1.1rem] max-[760px]:text-[0.95rem]">"acme-core"</strong>
								<span class="block text-[0.8rem] font-normal text-mute"><BrandIcon name="rust" />"Cargo"</span>
							</th>
							<td class="whitespace-nowrap text-[0.95rem] max-[760px]:text-[0.85rem]">{move || core_versions(shared.get()).0}</td>
							<td class="whitespace-nowrap text-[0.95rem] font-bold text-brand-700 max-[760px]:text-[0.85rem]">{move || core_versions(shared.get()).1}</td>
						</tr>
						<tr class="border-b border-rule">
							<th scope="row" class="py-[1.15rem] max-[760px]:py-[0.85rem]">
								<strong class="block text-[1.1rem] max-[760px]:text-[0.95rem]">"acme-mobile"</strong>
								<span class="block text-[0.8rem] font-normal text-mute"><BrandIcon name="dart" />"Dart"</span>
							</th>
							<td class="whitespace-nowrap text-[0.95rem] max-[760px]:text-[0.85rem]">{move || mobile_versions(shared.get()).0}</td>
							<td class="whitespace-nowrap text-[0.95rem] font-bold text-brand-700 max-[760px]:text-[0.85rem]">{move || mobile_versions(shared.get()).1}</td>
						</tr>
					</tbody>
				</table>
			</div>
			<p class="mt-[1.2rem] min-h-14 max-w-[48ch] text-[0.85rem] text-mute">
				{move || strategy_copy(shared.get())}
			</p>
			<div class="flex justify-between gap-4 pt-3 text-[0.85rem] font-bold text-brand-700">
				<span>"Preview first."</span>
				<span>"Publish when ready."</span>
			</div>
		</div>
	}
}

#[cfg(test)]
#[path = "__tests__/home_tests.rs"]
mod tests;
