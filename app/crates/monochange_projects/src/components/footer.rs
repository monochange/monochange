//! Shared public footer with verified destinations.
//!
//! Both browser entries render this one footer, so a page in the projects
//! workspace ends the same way as a page on the website.

use leptos::prelude::*;
use leptos_router::hooks::use_location;
use monochange_shared::routing::cross_surface_target;

use crate::components::brand_icon::BrandIcon;

/// Source repository and contribution guide.
pub const SOURCE_URL: &str = "https://github.com/monochange/monochange";

/// Site destinations in footer order.
const LINKS: [(&str, &str); 4] = [
	("/install", "Install"),
	("/book", "Read the book"),
	("/changelog", "Changelog"),
	("/pricing", "It's free"),
];

/// Public footer. `website_version` links the deployed website release to
/// its changelog; only the website entry knows that version, so other
/// entries leave it out.
#[component]
pub fn Footer(#[prop(optional)] website_version: Option<&'static str>) -> impl IntoView {
	let location = use_location();
	let target = move |href: &str| cross_surface_target(&location.pathname.get(), href);

	view! {
		<footer class="border-t border-rule py-12">
			<div class="mx-auto grid w-[min(80rem,calc(100%-6rem))] grid-cols-[1fr_auto_1fr] items-start gap-12 max-[1050px]:w-[calc(100%-4rem)] max-[760px]:w-[calc(100%-2.5rem)] max-[760px]:grid-cols-1 max-[760px]:gap-8">
				<div>
					<a href="/" target=move || target("/") class="inline-flex items-center gap-3 text-2xl font-bold tracking-[-0.035em] no-underline" aria-label="monochange home">
						<img src="/branding/mark.svg" width="36" height="36" alt="" class="shrink-0" />
						<img src="/branding/wordmark.svg" width="154" height="29" alt="" class="h-auto w-[154px] dark:hidden" />
						<img src="/branding/wordmark-dark.svg" width="154" height="29" alt="" class="hidden h-auto w-[154px] dark:block" />
					</a>
					<p class="mt-2.5 text-[0.95rem] text-mute">"Ship every package from one floor."</p>
				</div>
				<nav class="grid grid-cols-2 gap-x-8 gap-y-2.5" aria-label="Footer navigation">
					{LINKS.map(|(href, label)| view! {
						<a href=href target=move || target(href) class="hover:text-brand-600 dark:hover:text-brand-400">{label}</a>
					})}
					<a href=SOURCE_URL class="inline-flex items-center gap-2.5 hover:text-brand-600 dark:hover:text-brand-400">
						<BrandIcon name="github" />
						"GitHub"
					</a>
				</nav>
				<p class="text-right text-mute max-[760px]:text-left">
					"Free and open source." <br /> "Made by " <a href="https://github.com/ifiokjr" class="underline">"Ifiok Jr."</a>
					{website_version.map(|version| view! {
						<br />
						<a href="/changelog" target=move || target("/changelog") class="underline">{format!("Website v{version}")}</a>
					})}
				</p>
			</div>
		</footer>
	}
}

#[cfg(test)]
#[path = "__tests__/footer_tests.rs"]
mod tests;
