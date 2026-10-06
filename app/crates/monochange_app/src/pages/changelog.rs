//! Public website release history, rendered from generated user notes.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::server_fns::releases::WebsiteRelease;
use crate::server_fns::releases::get_website_releases;

/// Show approved website releases and a reusable integration example.
#[component]
pub fn ChangelogPage() -> impl IntoView {
	let releases = Resource::new(|| (), |()| get_website_releases());
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
					{move || releases.get().map(release_history)}
				</Suspense>
			</div>
			<aside class="release-example"><img src="/branding/mark.svg" width="64" height="64" alt="" /><h2>"Your notes. Your layout."</h2><p>"This page uses monochange's generated JSON. Use the same release notes in your own website, app, or update feed."</p><a class="text-link" href="https://monochange.github.io/monochange/guide/website-release-notes.html">"Build your own changelog"</a></aside>
		</section>
	}
}

fn release_history(result: Result<Vec<WebsiteRelease>, server_fn::ServerFnError>) -> AnyView {
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

#[cfg(test)]
#[path = "__tests__/changelog_tests.rs"]
mod tests;
