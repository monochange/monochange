//! Document shell. The page tree is the leptoskit file router.

use leptos::prelude::*;
use leptos_meta::MetaTags;
use leptos_meta::Stylesheet;
use leptos_meta::Title;
use leptos_meta::provide_meta_context;

use crate::color_mode::provide_color_mode;
use crate::navigation::current;

// Leptos strips literal comments. The inert template below preserves this
// contract in server output and occupies the same node during hydration.
const WEBSITE_DIRECTION: &str = "<!--
THESIS: Brand-first Release floor — ship every package from one calm workspace.
OWN-WORLD: Mist/graphite grounds, teal accent, hero atmosphere, Bricolage + Hanken, --mc-* tokens.
STORY: Open the workspace or install the CLI; prove product surfaces before the release-plan demo.
FIRST VIEWPORT: Full-bleed hero with mark + wordmark, one headline, one sentence, CTAs. No cards or demo in the hero.
FORM: Product floor → release manifest demo → ecosystems → workflow → close.
FINISH: Align with docs/landing-design.md; keep auth and dashboard routes intact.
-->";

/// Server-rendered document shell.
pub fn shell(options: LeptosOptions) -> impl IntoView {
	view! {
		<!DOCTYPE html>
		<html lang="en">
			<head>
				<meta charset="utf-8" />
				<meta name="viewport" content="width=device-width, initial-scale=1.0" />
				<meta name="theme-color" content="#0F766E" />
				<meta
					name="description"
					content="Free, open-source release planning for monorepos. Coordinate projects, releases, feedback, and org workspaces across six ecosystems."
				/>
				<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
				<link rel="icon" sizes="any" href="/favicon.ico" />
				<link rel="manifest" href="/site.webmanifest" />
				<meta property="og:image" content="https://monochange.dev/branding/social.png" />
				<meta property="og:image:alt" content="monochange — release planning for monorepos" />
				<meta name="twitter:card" content="summary_large_image" />
				<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png" />
				<AutoReload options=options.clone() />
				<HydrationScripts options=options />
				<MetaTags />
			</head>
			<body>
				<App />
			</body>
		</html>
	}
}

/// Root view: one leptoskit navigation, rendered into the document.
#[component]
pub fn App() -> impl IntoView {
	provide_meta_context();
	let _ = provide_color_mode();
	let navigation = current();
	let rendered = navigation.clone();

	view! {
		<template id="website-direction" inner_html=WEBSITE_DIRECTION />
		<Stylesheet id="leptos" href="/pkg/monochange_app.css" />
		<Title text="monochange — Release planning for monorepos" />
		<main id="main-content">
			<Suspense fallback=|| ()>
				{move || leptoskit::web::render(rendered.render())}
			</Suspense>
		</main>
	}
}

#[cfg(test)]
#[path = "__tests__/app_tests.rs"]
mod tests;
