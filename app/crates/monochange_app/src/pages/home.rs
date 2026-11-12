//! Public homepage: a working release plan instead of a product tour.
//!
//! The hero is a live plan console. Choosing a changed package and a bump
//! rewrites the changeset file and the release plan beside it, including the
//! dependents that have to release with it. Everything outside the console is
//! static markup so the wasm entry stays small.
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
const TEXT_LINK: &str = "inline-flex items-center gap-2 py-[0.65rem] font-bold text-brand-600 underline dark:text-brand-400";
const DISPLAY: &str = "font-display text-[clamp(2.4rem,3.2vw,3.4rem)] font-extrabold tracking-[-0.035em] leading-[1.06]";
const PANEL: &str = "min-w-0 rounded-2xl border border-[color-mix(in_srgb,var(--mc-hero-fg)_14%,transparent)] bg-[color-mix(in_srgb,var(--mc-hero-fg)_6%,transparent)] p-4";
const CHIP: &str = "mt-4 overflow-x-auto rounded-lg border border-rule bg-surface px-3 py-2.5";

/// One semver bump a changeset can request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bump {
	Patch,
	Minor,
	Major,
}

impl Bump {
	fn label(self) -> &'static str {
		match self {
			Self::Patch => "patch",
			Self::Minor => "minor",
			Self::Major => "major",
		}
	}
}

const BUMPS: [Bump; 3] = [Bump::Patch, Bump::Minor, Bump::Major];

/// The packages a visitor can change in the plan console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangedPackage {
	Core,
	Sdk,
	Mobile,
}

impl ChangedPackage {
	fn id(self) -> &'static str {
		match self {
			Self::Core => "acme-core",
			Self::Sdk => "@acme/sdk",
			Self::Mobile => "acme_mobile",
		}
	}
}

const CHOICES: [ChangedPackage; 3] = [
	ChangedPackage::Core,
	ChangedPackage::Sdk,
	ChangedPackage::Mobile,
];

/// One package in the illustrative workspace.
struct Package {
	id: &'static str,
	brand: &'static str,
	ecosystem: &'static str,
	version: (u32, u32, u32),
	depends_on: Option<&'static str>,
}

static PACKAGES: [Package; 6] = [
	Package {
		id: "acme-core",
		brand: "rust",
		ecosystem: "Cargo",
		version: (1, 8, 1),
		depends_on: None,
	},
	Package {
		id: "acme-cli",
		brand: "rust",
		ecosystem: "Cargo",
		version: (2, 0, 4),
		depends_on: Some("acme-core"),
	},
	Package {
		id: "@acme/sdk",
		brand: "npm",
		ecosystem: "npm",
		version: (3, 2, 0),
		depends_on: None,
	},
	Package {
		id: "@acme/ui",
		brand: "npm",
		ecosystem: "npm",
		version: (1, 4, 2),
		depends_on: Some("@acme/sdk"),
	},
	Package {
		id: "acme_mobile",
		brand: "dart",
		ecosystem: "Dart",
		version: (2, 1, 0),
		depends_on: None,
	},
	Package {
		id: "acme-py",
		brand: "python",
		ecosystem: "Python",
		version: (0, 9, 3),
		depends_on: None,
	},
];

/// How a package appears in the plan for the current selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowStatus {
	Changed(Bump),
	Dependent,
	Unchanged,
}

/// Standard semver arithmetic: a major bump clears minor and patch.
fn bump_version(version: (u32, u32, u32), bump: Bump) -> (u32, u32, u32) {
	let (major, minor, patch) = version;
	match bump {
		Bump::Patch => (major, minor, patch + 1),
		Bump::Minor => (major, minor + 1, 0),
		Bump::Major => (major + 1, 0, 0),
	}
}

/// Render a version as `major.minor.patch`.
fn format_version((major, minor, patch): (u32, u32, u32)) -> String {
	format!("{major}.{minor}.{patch}")
}

/// A selected package changes with the chosen bump; a direct dependent takes
/// monochange's default parent-bump floor of a patch release.
fn row_status(package: &Package, selected: ChangedPackage, bump: Bump) -> RowStatus {
	if package.id == selected.id() {
		RowStatus::Changed(bump)
	} else if package.depends_on == Some(selected.id()) {
		RowStatus::Dependent
	} else {
		RowStatus::Unchanged
	}
}

/// Whether the package releases at all for the selection.
fn affected(package: &Package, selected: ChangedPackage) -> bool {
	package.id == selected.id() || package.depends_on == Some(selected.id())
}

/// Footer copy for the plan, e.g. "2 of 6 packages release · nothing published yet".
fn plan_summary(selected: ChangedPackage) -> String {
	let released = PACKAGES
		.iter()
		.filter(|package| affected(package, selected))
		.count();
	format!(
		"{released} of {} packages release · nothing published yet",
		PACKAGES.len()
	)
}

/// The changeset filename for the selected package.
fn changeset_slug(selected: ChangedPackage) -> &'static str {
	match selected {
		ChangedPackage::Core => "quiet-otters",
		ChangedPackage::Sdk => "brave-lanterns",
		ChangedPackage::Mobile => "tidy-comets",
	}
}

/// The release note inside the generated changeset.
fn changeset_note(selected: ChangedPackage) -> &'static str {
	match selected {
		ChangedPackage::Core => "Faster dependency resolution",
		ChangedPackage::Sdk => "Typed release events",
		ChangedPackage::Mobile => "Offline release notes",
	}
}

/// Version shown in the "next" column; unchanged packages show a dash.
fn next_version_text(package: &Package, status: RowStatus) -> String {
	match status {
		RowStatus::Changed(bump) => format_version(bump_version(package.version, bump)),
		RowStatus::Dependent => format_version(bump_version(package.version, Bump::Patch)),
		RowStatus::Unchanged => "—".to_owned(),
	}
}

/// Tailwind classes that express a row status.
struct RowStyle {
	row: &'static str,
	leading: &'static str,
	next: &'static str,
	tag_class: &'static str,
	tag_label: Option<&'static str>,
}

fn row_style(status: RowStatus) -> RowStyle {
	match status {
		RowStatus::Changed(bump) => {
			RowStyle {
				row: "bg-brand-400/10",
				leading: "border-l-2 border-brand-300",
				next: "font-bold text-brand-300",
				tag_class: "border border-signal/70 text-signal",
				tag_label: Some(bump.label()),
			}
		}
		RowStatus::Dependent => {
			RowStyle {
				row: "bg-brand-400/5",
				leading: "border-l-2 border-brand-500/60",
				next: "text-brand-300",
				tag_class: "border border-brand-300/40 text-brand-300",
				tag_label: Some("dependency"),
			}
		}
		RowStatus::Unchanged => {
			RowStyle {
				row: "",
				leading: "",
				next: "text-hero-muted",
				tag_class: "",
				tag_label: None,
			}
		}
	}
}

/// Pill styling for the console's pressed/unpressed choice buttons.
fn pill_class(pressed: bool) -> String {
	format!(
		"inline-flex min-h-11 items-center rounded-full px-3.5 font-mono text-[0.78rem] transition-colors {}",
		if pressed {
			"bg-brand-300 text-brand-950"
		} else {
			"text-hero-muted hover:bg-[color-mix(in_srgb,var(--mc-hero-fg)_10%,transparent)] hover:text-mist"
		}
	)
}

fn select_package(set_selected: WriteSignal<ChangedPackage>, choice: ChangedPackage) {
	set_selected.set(choice);
}

fn select_bump(set_bump: WriteSignal<Bump>, bump: Bump) {
	set_bump.set(bump);
}

/// Path for the hero/workspace CTA from the current session resource value.
fn workspace_href_for(
	session: Option<Result<Option<SessionUser>, server_fn::ServerFnError>>,
) -> String {
	session.and_then(Result::ok).flatten().map_or_else(
		|| "/login?next=%2Fprojects".to_owned(),
		|_| "/projects".to_owned(),
	)
}

/// `target="_self"` when the workspace link lands in the projects entry, so
/// the click loads that browser entry as a document navigation.
fn workspace_target(workspace_href: &Memo<String>) -> Option<&'static str> {
	monochange_shared::routing::cross_surface_target("/", &workspace_href.get_untracked())
}

/// Explain the release workspace before asking visitors to install it.
#[component]
pub fn HomePage() -> impl IntoView {
	let session = Resource::new_blocking(|| (), |()| get_session());
	let workspace_href = Memo::new(move |_| workspace_href_for(session.get()));

	view! {
		<Title text="monochange — Release planning for monorepos" />
		<HomeHero workspace_href=workspace_href />
		<WorkflowSection />
		<EcosystemSection />
		<WorkspaceSection workspace_href=workspace_href />
		<ClosingSection />
	}
}

/// Full-bleed hero: the promise on the left, the live plan console on the right.
#[component]
fn HomeHero(workspace_href: Memo<String>) -> impl IntoView {
	view! {
		<section class="relative overflow-hidden bg-hero text-mist" aria-label="monochange">
			<div class=format!(
				"{SITE} grid min-h-[min(100svh,60rem)] grid-cols-1 content-center items-center gap-12 py-[clamp(3rem,7vh,5rem)] min-[1050px]:grid-cols-[minmax(0,1fr)_minmax(0,32rem)] min-[1050px]:gap-12 max-[760px]:gap-8"
			)>
				<div>
					<p class="font-mono text-[0.78rem] uppercase tracking-[0.18em] text-signal">
						"release planning for monorepos"
					</p>
					<h1 class="mt-5 max-w-[13ch] font-display text-[clamp(3.1rem,4.6vw,4.75rem)] font-extrabold tracking-[-0.035em] leading-[1.06] max-[760px]:text-[clamp(2.4rem,9vw,3.6rem)]">
						"Change one package. See every release it touches."
					</h1>
					<p class="mt-6 max-w-[42ch] text-[1.2rem] text-hero-muted max-[760px]:mt-4 max-[760px]:text-[1.05rem]">
						"monochange turns a changeset into a release plan across Cargo, npm, Dart, Python, Go, and Deno — dependents, versions, and changelogs — before anything ships."
					</p>
					<div class="mt-8 flex flex-wrap items-center gap-3 max-[760px]:mt-6">
						<a
							href=move || workspace_href.get()
							target=move || workspace_target(&workspace_href)
							class=format!("{BTN} {BTN_LIGHT}")
						>
							"Open workspace"
							<ArrowIcon />
						</a>
						<a href="/install" class=format!("{BTN} {BTN_OUTLINE}")>
							"Install CLI"
						</a>
					</div>
					<a href="/pricing" class="mt-5 inline-block text-[0.9rem] text-hero-muted underline">
						"Free and open source."
					</a>
				</div>
				<PlanConsole />
			</div>
		</section>
	}
}

/// The hero's one interactive surface: a changeset that rewrites the plan.
#[component]
fn PlanConsole() -> impl IntoView {
	let (selected, set_selected) = signal(ChangedPackage::Core);
	let (bump, set_bump) = signal(Bump::Minor);

	view! {
		<div class=PANEL>
			<div class="flex items-center gap-2 font-mono text-[0.78rem] text-hero-muted">
				<span class="size-1.5 rounded-full bg-signal" aria-hidden="true"></span>
				{move || format!(".changeset/{}.md", changeset_slug(selected.get()))}
			</div>
			<pre class="mt-3 whitespace-pre-wrap break-words rounded-xl border border-[color-mix(in_srgb,var(--mc-hero-fg)_10%,transparent)] bg-black/25 p-3 font-mono text-[0.78rem] leading-[1.5] text-mist">
				"---\n"
				<span class="text-brand-300">{move || selected.get().id()}</span>
				": "
				<span class="text-signal">{move || bump.get().label()}</span>
				"\n---\n\n# "
				{move || changeset_note(selected.get())}
			</pre>
			<div class="mt-3 flex flex-wrap gap-x-6 gap-y-2">
				<div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Changed package">
					{CHOICES
						.iter()
						.map(|choice| {
							let choice = *choice;
							view! {
								<button
									type="button"
									class=move || pill_class(selected.get() == choice)
									aria-pressed=move || (selected.get() == choice).to_string()
									on:click=move |_| select_package(set_selected, choice)
								>
									{choice.id()}
								</button>
							}
						})
						.collect_view()}
				</div>
				<div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Bump">
					{BUMPS
						.iter()
						.map(|choice| {
							let choice = *choice;
							view! {
								<button
									type="button"
									class=move || pill_class(bump.get() == choice)
									aria-pressed=move || (bump.get() == choice).to_string()
									on:click=move |_| select_bump(set_bump, choice)
								>
									{choice.label()}
								</button>
							}
						})
						.collect_view()}
				</div>
			</div>
			<p class="mt-6 border-t border-[color-mix(in_srgb,var(--mc-hero-fg)_10%,transparent)] pt-5 font-mono text-[0.78rem] uppercase tracking-[0.14em] text-hero-muted">
				"monochange preview"
			</p>
			<div class="mt-3 overflow-x-auto" aria-live="polite">
				<table class="w-full min-w-[21rem] border-collapse text-left">
					<caption class="sr-only">
						"Illustrative release plan for an example workspace"
					</caption>
					<thead>
						<tr>
							<th scope="col" class="sr-only">"Package"</th>
							<th scope="col" class="sr-only">"Current version"</th>
							<th scope="col" class="sr-only"></th>
							<th scope="col" class="sr-only">"Next version"</th>
							<th scope="col" class="sr-only">"Status"</th>
						</tr>
					</thead>
					<tbody>
						{move || {
							PACKAGES
								.iter()
								.map(|package| {
									let status = row_status(package, selected.get(), bump.get());
									let style = row_style(status);
									let next = next_version_text(package, status);
									view! {
										<tr class=format!(
											"border-b border-[color-mix(in_srgb,var(--mc-hero-fg)_10%,transparent)] last:border-0 {}",
											style.row,
										)>
											<th
												scope="row"
												class=format!(
													"py-2 pl-3 pr-3 text-left font-normal {}",
													style.leading,
												)
											>
												<span class="flex items-center gap-2.5">
													<BrandIcon name=package.brand />
													<strong class="whitespace-nowrap text-[0.92rem] font-bold text-mist">
														{package.id}
													</strong>
													<span class="text-[0.7rem] text-hero-muted max-[760px]:sr-only">
														{package.ecosystem}
													</span>
												</span>
											</th>
											<td class="whitespace-nowrap py-2 pr-2 font-mono text-[0.8rem] text-hero-muted">
												{format_version(package.version)}
											</td>
											<td class="pr-2 text-hero-muted" aria-hidden="true">
												"→"
											</td>
											<td class=format!(
												"whitespace-nowrap py-2 pr-3 font-mono text-[0.8rem] {}",
												style.next,
											)>
												<span class="inline-block animate-[version-roll_0.45s_cubic-bezier(0.16,1,0.3,1)] motion-reduce:animate-none">
													{next}
												</span>
											</td>
											<td class="py-2 pr-3 text-right">
												{style
													.tag_label
													.map(|label| {
														view! {
															<span class=format!(
																"inline-flex items-center rounded-full px-2 py-0.5 font-mono text-[0.62rem] uppercase tracking-[0.08em] {}",
																style.tag_class,
															)>{label}</span>
														}
													})}
											</td>
										</tr>
									}
								})
								.collect_view()
						}}
					</tbody>
				</table>
			</div>
			<p class="mt-4 font-mono text-[0.75rem] text-hero-muted">
				{move || plan_summary(selected.get())}
			</p>
		</div>
	}
}

/// Three commands, from a recorded change to a published release.
#[component]
fn WorkflowSection() -> impl IntoView {
	const STEPS: [(&str, &str, &str); 3] = [
		(
			"Describe the change",
			"Record which package changed, how much, and why.",
			"monochange create --package acme-core --bump minor",
		),
		(
			"Preview the plan",
			"See versions, dependents, and changelog diffs without touching a file.",
			"monochange preview --diff",
		),
		(
			"Prepare the release",
			"Write versions and changelogs, then publish from CI when you're ready.",
			"monochange prepare",
		),
	];

	view! {
		<section class=format!("{SITE} py-[6.5rem] max-[760px]:py-16")>
			<div class="grid max-w-4xl gap-4">
				<h2 class=DISPLAY>"Three commands. One decision."</h2>
				<p class="max-w-[50ch] text-[1.15rem] text-mute">
					"Changesets live beside the code they describe. Nothing is written until you prepare, and nothing ships until you publish."
				</p>
			</div>
			<ol class="mt-12 grid list-none grid-cols-3 gap-10 border-t border-rule max-[1050px]:gap-6 max-[760px]:grid-cols-1 max-[760px]:gap-8">
				{STEPS
					.iter()
					.enumerate()
					.map(|(index, (title, copy, command))| {
						view! {
							<li class="pt-8">
								<span class="font-mono text-[0.8rem] font-bold tracking-wider text-brand-600 dark:text-brand-400">
									{format!("{:02}", index + 1)}
								</span>
								<h3 class="mt-3 text-[1.35rem] font-bold">{*title}</h3>
								<p class="mt-2 max-w-[34ch] text-[0.98rem] text-mute">{*copy}</p>
								<div class=CHIP>
									<code class="whitespace-nowrap font-mono text-[0.75rem]">{*command}</code>
								</div>
							</li>
						}
					})
					.collect_view()}
			</ol>
			<a href=BOOK_URL class=format!("mt-10 {TEXT_LINK}")>
				"Follow your first release plan"
				<ArrowIcon />
			</a>
		</section>
	}
}

/// The ecosystems a workspace can span, kept as a ruled row rather than cards.
#[component]
fn EcosystemSection() -> impl IntoView {
	const ECOSYSTEMS: [(&str, &str, &str); 6] = [
		("rust", "Cargo", "Rust crates"),
		("npm", "npm", "JS & TS packages"),
		("dart", "Dart", "Dart & Flutter"),
		("python", "Python", "Python packages"),
		("go", "Go", "Go modules"),
		("deno", "Deno", "Deno & JSR"),
	];

	view! {
		<section class=format!("{SITE} pb-[6.5rem] max-[760px]:pb-16")>
			<div class="grid max-w-4xl gap-4">
				<h2 class=DISPLAY>"Your tools stay. The plan is shared."</h2>
			</div>
			<ul
				class="mt-12 grid grid-cols-6 max-[1050px]:grid-cols-3 max-[760px]:grid-cols-2"
				aria-label="Supported package ecosystems"
			>
				{ECOSYSTEMS
					.iter()
					.map(|(brand, name, caption)| {
						view! {
							<li class="border-l border-rule px-5 py-2 max-[760px]:px-4">
								<span class="inline-flex text-[1.7rem] text-brand-600 dark:text-brand-400">
									<BrandIcon name=*brand />
								</span>
								<strong class="mt-3 block text-[1.05rem] font-bold">{*name}</strong>
								<span class="mt-1 block text-[0.82rem] text-mute">{*caption}</span>
							</li>
						}
					})
					.collect_view()}
			</ul>
		</section>
	}
}

/// What a signed-in workspace adds; a dark band so it reads as a different room.
#[component]
fn WorkspaceSection(workspace_href: Memo<String>) -> impl IntoView {
	const SURFACES: [(&str, &str); 4] = [
		("Projects", "Group repositories under one release identity."),
		("Releases", "Preview versions and notes before you publish."),
		(
			"Feedback",
			"A contributor portal and maintainer console in one place.",
		),
		(
			"Organizations",
			"Connected orgs, installations, and permissions.",
		),
	];

	view! {
		<section class="bg-ink py-[6.5rem] text-mist dark:bg-hero max-[760px]:py-16">
			<div class=format!(
				"{SITE} grid grid-cols-2 items-start gap-16 max-[1050px]:gap-10 max-[760px]:grid-cols-1 max-[760px]:gap-8"
			)>
				<div>
					<h2 class=DISPLAY>"One sign-in. Every surface."</h2>
					<p class="mt-5 max-w-[42ch] text-[1.1rem] text-hero-muted">
						"Sign in once with GitHub and the site, your projects, and your connected repositories all know it's you."
					</p>
					<a
						href=move || workspace_href.get()
						target=move || workspace_target(&workspace_href)
						class=format!("mt-8 {BTN} {BTN_LIGHT}")
					>
						"Open workspace"
					</a>
				</div>
				<ul class="list-none">
					{SURFACES
						.iter()
						.enumerate()
						.map(|(index, (title, copy))| {
							view! {
								<li class="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-4 border-b border-mist/15 py-4 first:pt-0">
									<span class="font-mono text-[0.8rem] text-brand-400">
										{format!("{:02}", index + 1)}
									</span>
									<span>
										<strong class="block text-[1.1rem] font-bold">{*title}</strong>
										<span class="mt-1 block text-[0.95rem] text-hero-muted">{*copy}</span>
									</span>
								</li>
							}
						})
						.collect_view()}
				</ul>
			</div>
		</section>
	}
}

/// Closing offer: free, open source, and one command away.
#[component]
fn ClosingSection() -> impl IntoView {
	view! {
		<section class=format!("{SITE} py-24 text-center max-[760px]:py-16")>
			<img src="/branding/mark.svg" width="64" height="64" alt="" class="mx-auto size-16" />
			<h2 class=format!("mt-8 {DISPLAY}")>"Hey, this is free."</h2>
			<p class="mx-auto mt-5 max-w-[46ch] text-[1.1rem] text-mute">
				"No tiers, no seats. Install the CLI and plan your next release."
			</p>
			<div class="mt-8 flex flex-wrap items-center justify-center gap-4">
				<a href="/install" class=format!("{BTN} {BTN_BRAND}")>
					"Install the CLI"
					<ArrowIcon />
				</a>
			</div>
			<div class="mt-6 flex flex-wrap items-center justify-center gap-x-8">
				<a href=BOOK_URL class=TEXT_LINK>"Read the book"</a>
				<a href=SOURCE_URL class=TEXT_LINK>"Explore the source"</a>
			</div>
		</section>
	}
}

#[cfg(test)]
#[path = "__tests__/home_tests.rs"]
mod tests;
