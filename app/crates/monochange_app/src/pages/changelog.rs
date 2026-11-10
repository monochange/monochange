//! Public website release history, rendered from generated user notes.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::LazyRoute;
use leptos_router::lazy_route;

use crate::server_fns::releases::WebsiteRelease;
use crate::server_fns::releases::get_website_releases;

/// Changelog data, split into its own WASM chunk.
pub struct ChangelogRoute {
	releases: Resource<Result<Vec<WebsiteRelease>, server_fn::ServerFnError>>,
}

/// Show approved website releases and a reusable integration example.
#[lazy_route]
impl LazyRoute for ChangelogRoute {
	fn data() -> Self {
		Self {
			releases: Resource::new(|| (), |()| get_website_releases()),
		}
	}

	fn view(this: Self) -> AnyView {
		let ChangelogRoute { releases } = this;
		changelog_route_view(releases)
	}
}

fn changelog_route_view(
	releases: Resource<Result<Vec<WebsiteRelease>, server_fn::ServerFnError>>,
) -> AnyView {
	view! {
		<Title text="What's new — monochange" />
		<section class="site-width release-intro">
			<p class="eyebrow">"The website, version by version"</p>
			<h1>"What's new."</h1>
			<p>"Updates that make monochange.dev better for you. The CLI has its own "<a href="https://github.com/monochange/monochange/releases">"release notes"</a>"."</p>
		</section>
		<section class="site-width release-layout" aria-label="Website release history">
			<div class="release-history">
				<Suspense fallback=|| view! { <p role="status">"Loading release notes…"</p> }>
					{move || releases.get().map(|result| view! { <ReleaseHistory result=result /> })}
				</Suspense>
			</div>
			<aside class="release-example"><img src="/branding/mark.svg" width="64" height="64" alt="" /><h2>"Your notes. Your layout."</h2><p>"This page uses monochange's generated JSON. Use the same release notes in your own website, app, or update feed."</p><a class="text-link" href="/book/guide/website-release-notes">"Build your own changelog"</a></aside>
		</section>
	}
	.into_any()
}

#[component]
fn ReleaseHistory(result: Result<Vec<WebsiteRelease>, ServerFnError>) -> impl IntoView {
	match result {
		Err(error) => view! { <p role="alert">{error.to_string()}</p> }.into_any(),

		Ok(releases) if releases.is_empty() => {
			view! {
				<div class="release-empty">
					<h2>"Our first versioned release is on its way."</h2>
					<p>"This page fills with published website updates as they ship."</p>
				</div>
			}
			.into_any()
		}

		Ok(releases) => {
			releases
				.into_iter()
				.map(|release| {
					view! {
						<article class="release-card" id=format!("v{}", release.version)>
							<div class="release-heading">
								<p class="eyebrow">"Website release"</p>
								<h2>{format!("v{}", release.version)}</h2>
								<a class="text-link" href=format!("/releases/{}.json", release.version)>"View JSON"</a>
							</div>
							{release.summary.into_iter().map(|summary| view! { <p>{summary}</p> }).collect_view()}
							{release.sections.into_iter().map(|section| view! {
								<section class="release-section">
									<h3>{section.title}</h3>
									<ul>{section.entries.into_iter().map(|entry| view! {
										<li>
											<h4>{entry.summary}</h4>
											<div class="release-details" inner_html=entry.details_html />
										</li>
									}).collect_view()}</ul>
								</section>
							}).collect_view()}
						</article>
					}
				})
				.collect_view()
				.into_any()
		}
	}
}

/// Changelog screen.
pub fn changelog_screen() -> leptoskit::prelude::Element {
	let releases =
		crate::slots::once(|| Resource::new_blocking(|| (), |()| get_website_releases()));

	match releases.get() {
		None => crate::ui::text("Loading release notes…"),
		Some(Err(error)) => crate::ui::text(error.to_string()),
		Some(Ok(releases)) => changelog_content(releases),
	}
}

fn changelog_content(releases: Vec<WebsiteRelease>) -> leptoskit::prelude::Element {
	let mut children = vec![
		crate::ui::heading(1, "What's new."),
		crate::ui::text(
			"Updates that make monochange.dev better for you. The CLI has its own release notes.",
		),
		crate::ui::link(
			"https://github.com/monochange/monochange/releases",
			"release notes",
		),
		crate::ui::heading(2, "Your notes. Your layout."),
		crate::ui::link(
			"/book/guide/website-release-notes",
			"Build your own changelog",
		),
	];

	if releases.is_empty() {
		children.push(crate::ui::heading(
			2,
			"Our first versioned release is on its way.",
		));
		children.push(crate::ui::text(
			"This page fills with published website updates as they ship.",
		));
	}

	for release in releases {
		children.push(crate::ui::heading(2, format!("v{}", release.version)));
		children.push(crate::ui::link(
			&format!("/releases/{}.json", release.version),
			"View JSON",
		));

		for summary in release.summary {
			children.push(crate::ui::text(summary));
		}

		for section in release.sections {
			children.push(crate::ui::heading(3, section.title));

			for entry in section.entries {
				children.push(crate::ui::text(entry.summary));
				let details = plain_text(&entry.details_html);

				if !details.trim().is_empty() {
					children.push(crate::ui::text(details));
				}
			}
		}
	}

	crate::ui::with_children(crate::ui::column("flex flex-col gap-3 p-6"), children)
}

fn plain_text(html: &str) -> String {
	let mut text = String::new();
	let mut inside = false;

	for character in html.chars() {
		match character {
			'<' => inside = true,
			'>' => {
				inside = false;
				text.push(' ');
			}
			_ if !inside => text.push(character),
			_ => {}
		}
	}

	text
}

#[cfg(test)]
#[path = "__tests__/changelog_tests.rs"]
mod tests;
