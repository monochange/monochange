//! Public navigation with accessible mobile controls and persistent color mode.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::color_mode::ColorMode;
use crate::color_mode::provide_color_mode;
use crate::color_mode::use_color_mode;
use crate::links::BOOK_URL;

/// Public navigation, including a direct link to the published book.
#[component]
pub fn NavBar() -> impl IntoView {
	let _ = provide_color_mode();
	let color_mode = use_color_mode();
	let (mobile_open, set_mobile_open) = signal(false);
	let location = use_location();

	Effect::new(move || {
		location.pathname.get();
		set_mobile_open.set(false);
	});

	view! {
		<a href="#main-content" class="skip-link">"Skip to content"</a>
		<header class="site-header">
			<nav class="site-width nav-layout" aria-label="Main navigation" on:keydown=move |event| {
				if event.key() == "Escape" { set_mobile_open.set(false); }
			}>
				<a href="/" class="brand-link" aria-label="monochange home"><img src="/branding/mark.svg" width="36" height="36" alt="" /><img src="/branding/wordmark.svg" width="154" height="29" alt="" class="brand-wordmark brand-wordmark-light" /><img src="/branding/wordmark-dark.svg" width="154" height="29" alt="" class="brand-wordmark brand-wordmark-dark" /></a>
				<div class="desktop-links">
					<a href="/install" aria-current=move || (location.pathname.get() == "/install").then_some("page")>"Install"</a>
					<a href=BOOK_URL>"Docs"</a>
					<a href="/pricing" aria-current=move || (location.pathname.get() == "/pricing").then_some("page")>"Pricing"</a>
				</div>
				<div class="nav-actions">
					<button class="theme-toggle" on:click=move |_| color_mode.toggle.run(()) aria-label=move || if color_mode.mode.get() == ColorMode::Dark { "Use light theme" } else { "Use dark theme" }>
						// Keep the SVG structure identical during SSR and hydration.
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" width="20" height="20" aria-hidden="true" class:hidden=move || color_mode.mode.get() == ColorMode::Dark>
							<path d="M20.5 14.3A9 9 0 0 1 9.7 3.5 9 9 0 1 0 20.5 14.3Z" />
						</svg>
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" width="20" height="20" aria-hidden="true" class:hidden=move || color_mode.mode.get() == ColorMode::Light>
							<circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5M17.5 17.5L19 19M5 19l1.5-1.5M17.5 6.5L19 5" />
						</svg>
					</button>
					<a href="/login" class="button button-brand nav-sign-in">"Sign in"</a>
					<button class="menu-toggle" aria-label="Toggle navigation menu" aria-expanded=move || mobile_open.get().to_string() aria-controls="mobile-navigation" on:click=move |_| set_mobile_open.update(|open| *open = !*open)>
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" width="22" height="22" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16" /></svg>
					</button>
				</div>
				<div id="mobile-navigation" class="mobile-links" hidden=move || !mobile_open.get()>
					<a href="/install">"Install"</a><a href=BOOK_URL>"Docs"</a><a href="/pricing">"Pricing"</a><a href="/login">"Sign in"</a>
				</div>
			</nav>
		</header>
	}
}
