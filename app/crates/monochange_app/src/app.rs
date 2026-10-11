//! Shared document shell, public routes, and GitHub sign-in pages.

use leptos::prelude::*;
use leptos_meta::MetaTags;
use leptos_meta::Stylesheet;
use leptos_meta::Title;
use leptos_meta::provide_meta_context;
use leptos_router::components::Route;
use leptos_router::components::Router;
use leptos_router::components::Routes;
use leptos_router::path;

use crate::components::brand_icon::BrandIcon;
use crate::components::navbar::NavBar;
use crate::error::ErrorTemplate;
use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::pages::book::BookPageView;
use crate::pages::changelog::ChangelogPage;
use crate::pages::dashboard::DashboardPage;
use crate::pages::home::HomePage;
use crate::pages::install::InstallPage;
use crate::pages::organization::OrganizationPage;
use crate::pages::pricing::PricingPage;
use crate::pages::project::ProjectPage;

// Leptos strips literal comments. The inert template below preserves this
// contract in server output and occupies the same node during hydration.
const WEBSITE_DIRECTION: &str = "<!--
THESIS: Show one concrete release plan, avoiding a grid of feature promises.
OWN-WORLD: Indigo fields, lavender and ink grounds, rounded display type, ruled version rows, and the selected flowing identity.
STORY: Understand coordinated releases, believe the preview, install the free CLI or open the book.
FIRST VIEWPORT: Desktop pairs large promise and actions with the full manifest; mobile stacks a compact introduction above readable package/version rows.
FORM: Shipping manifest, candidate 5 of seven; seed 19d35619. Switching individual/shared versions is the signature interaction.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
-->";

/// Server-rendered document shell.
pub fn shell(options: LeptosOptions) -> impl IntoView {
	view! {
		<!DOCTYPE html>
		<html lang="en">
			<head>
				<meta charset="utf-8" />
				<meta name="viewport" content="width=device-width, initial-scale=1.0" />
				<meta name="theme-color" content="#4f46e5" />
				<meta
					name="description"
					content="Free, open-source release planning for monorepos. Coordinate changesets, package versions, dependencies, and release notes across six ecosystems."
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

/// Root `<App/>` component.
#[component]
pub fn App() -> impl IntoView {
	provide_meta_context();

	view! {
		<template id="website-direction" inner_html=WEBSITE_DIRECTION />
		<Stylesheet id="leptos" href="/pkg/monochange_app.css" />
		<Title text="monochange — Release planning for monorepos" />
		<Router>
			<NavBar />
			<main id="main-content" class="min-h-screen">
				<Routes fallback=|| {
					view! { <ErrorTemplate status=404 message="Page not found" /> }
				}>
					<Route path=path!("/") view=HomePage />
					<Route path=path!("/dashboard") view=DashboardPage />
					<Route path=path!("/dashboard/:organization") view=OrganizationPage />
					<Route path=path!("/dashboard/:organization/projects/:project") view=ProjectPage />
					<Route path=path!("/install") view=InstallPage />
					<Route path=path!("/login") view=LoginPage />
					<Route path=path!("/pricing") view=PricingPage />
					<Route path=path!("/changelog") view=ChangelogPage />
					<Route path=path!("/book") view=BookPageView />
					<Route path=path!("/book/*chapter") view=BookPageView />
					<Route path=path!("/auth/callback") view=AuthCallbackPage />
				</Routes>
			</main>
			<Footer />
		</Router>
	}
}

// ── Login Page ──

#[component]
fn LoginPage() -> impl IntoView {
	let login_url = Resource::new_blocking(
		|| (),
		|()| {
			async {
				crate::server_fns::auth::get_login_url()
					.await
					.unwrap_or_default()
			}
		},
	);

	view! {
		<section class="login-section site-width">
			<div class="login-copy"><h1>"Your workspace," <br /> "all together."</h1><p>"Sign in to see your connected repositories and manage your monochange workspace."</p><a href=BOOK_URL class="text-link">"Just looking for the CLI? Read the book."</a></div>
			<div class="login-panel">
				// Logo mark
				<div class="login-mark">
					<img src="/branding/mark.svg" alt="monochange" class="size-12" />
				</div>

				<h2>
					Sign in to monochange
				</h2>
				<p class="login-description">
					Use your GitHub account to continue.
				</p>

				<div class="mt-10">
					<Suspense fallback=|| {
						view! {
							<div class="mx-auto h-12 w-64 animate-pulse rounded-xl bg-gray-100 dark:bg-gray-800" />
						}
					}>
						{move || login_url.get().map(|url| {
							if url.is_empty() {
								view! {
									<div class="rounded-xl border border-red-200 bg-red-50 p-4 dark:border-red-800 dark:bg-red-950">
										<p class="text-sm text-red-600 dark:text-red-400">
											GitHub sign-in is unavailable right now. Please try again later.
										</p>
									</div>
								}.into_any()
							} else {
								view! {
									<a
										href=url
										class="button button-brand"
									>
										<svg class="size-5" fill="currentColor" viewBox="0 0 16 16" aria-hidden="true">
											<path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
										</svg>
										Continue with GitHub
										<svg class="size-4 transition-transform group-hover:translate-x-0.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
											<path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
										</svg>
									</a>
								}.into_any()
							}
						})}
					</Suspense>
				</div>

				<p class="login-notice">
					"GitHub will show the account access requested before you continue."
				</p>
			</div>
		</section>
	}
}

// ── Auth Callback Page ──

#[component]
fn AuthCallbackPage() -> impl IntoView {
	let params = leptos_router::hooks::use_query_map();

	// Cookie headers must be complete before SSR starts sending the page.
	let result = Resource::new_blocking(
		move || params.get(),
		|params| {
			async move {
				let code = params.get("code").unwrap_or_default();
				let state = params.get("state").unwrap_or_default();
				if code.is_empty() {
					return Err("No authorization code received".to_string());
				}
				crate::server_fns::auth::exchange_code(code, state)
					.await
					.map_err(|e| e.to_string())
			}
		},
	);

	view! {
		<div class="flex min-h-[80vh] items-center justify-center">
			<Suspense fallback=|| {
				view! {
					<div class="text-center">
						<div class="mx-auto size-12 animate-spin rounded-full border-4 border-brand-200 border-t-brand-600" />
						<p class="mt-4 text-sm text-gray-500">Completing sign in...</p>
					</div>
				}
			}>
				{move || result.get().map(|r| match r {
					Ok(user) => view! {
						<div class="text-center">
							<div class="mx-auto mb-4 flex size-16 items-center justify-center rounded-full bg-green-100 dark:bg-green-900">
								<svg class="size-8 text-green-600 dark:text-green-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
									<path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5" />
								</svg>
							</div>
							<h2 class="text-2xl font-bold text-gray-900 dark:text-white">Signed in!</h2>
							<p class="mt-2 text-gray-600 dark:text-gray-400">Welcome, {user.github_login}!</p>
							<a href="/dashboard" class="mt-8 inline-flex items-center gap-x-2 rounded-xl bg-brand-600 px-6 py-3 text-sm font-semibold text-white shadow-sm hover:bg-brand-700 transition-colors">
								Go to dashboard
								<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
									<path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
								</svg>
							</a>
						</div>
					}.into_any(),
					Err(e) => view! {
						<div class="text-center">
							<div class="mx-auto mb-4 flex size-16 items-center justify-center rounded-full bg-red-100 dark:bg-red-900">
								<svg class="size-8 text-red-600 dark:text-red-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
									<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
								</svg>
							</div>
							<h2 class="text-2xl font-bold text-red-600 dark:text-red-400">Sign in failed</h2>
							<p class="mt-2 text-gray-600 dark:text-gray-400">{e}</p>
							<a href="/login" class="mt-8 inline-block rounded-xl bg-brand-600 px-6 py-3 text-sm font-semibold text-white hover:bg-brand-700 transition-colors">
								Try again
							</a>
						</div>
					}.into_any(),
				})}
			</Suspense>
		</div>
	}
}

/// Shared public footer with verified destinations.
#[component]
fn Footer() -> impl IntoView {
	view! {
		<footer class="site-footer">
			<div class="site-width footer-layout">
				<div><a href="/" class="brand-link" aria-label="monochange home"><img src="/branding/mark.svg" width="36" height="36" alt="" /><img src="/branding/wordmark.svg" width="154" height="29" alt="" class="brand-wordmark brand-wordmark-light" /><img src="/branding/wordmark-dark.svg" width="154" height="29" alt="" class="brand-wordmark brand-wordmark-dark" /></a><p>"Many packages. One release plan."</p></div>
				<nav aria-label="Footer navigation"><a href="/install">"Install"</a><a href=BOOK_URL>"Read the book"</a><a href="/changelog">"Changelog"</a><a href="/pricing">"It's free"</a><a href=SOURCE_URL class="footer-github"><BrandIcon name="github" />"GitHub"</a></nav>
				<p class="footer-credit">"Free and open source."<br />"Made by "<a href="https://github.com/ifiokjr">"Ifiok Jr."</a><br /><a href="/changelog">{format!("Website v{}", env!("CARGO_PKG_VERSION"))}</a></p>
			</div>
		</footer>
	}
}

#[cfg(test)]
#[path = "__tests__/app_tests.rs"]
mod tests;
