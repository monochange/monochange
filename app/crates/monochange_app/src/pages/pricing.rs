//! Free pricing, without paid tiers or purchase flows.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::arrow::ArrowIcon;
use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;

/// State the free product offering plainly.
#[component]
pub fn PricingPage() -> impl IntoView {
	view! {
		<Title text="It's free — monochange" />
		<section class="free-section">
			<div class="site-width free-layout">
				<div>
					<h1>"Hey," <br /> "this is free."</h1>
					<p>"Use monochange for your release planning. No paid plan to choose. No checkout to get through."</p>
					<div class="actions"><a href="/install" class="button button-light">"Install monochange" <ArrowIcon /></a><a href=BOOK_URL class="button button-outline">"Read the book"</a></div>
				</div>
				<img src="/branding/mark.svg" width="256" height="256" alt="" class="free-mark" />
			</div>
		</section>
		<section class="site-width free-details"><h2>"Open source, too."</h2><p>"Read the code, report an issue, or contribute an improvement. The CLI and application live in the same repository."</p><a href=SOURCE_URL class="text-link">"Visit monochange on GitHub" <ArrowIcon /></a></section>
	}
}
