//! Free pricing, without paid tiers or purchase flows.

use leptoskit::prelude::*;

use crate::links::BOOK_URL;
use crate::links::SOURCE_URL;
use crate::ui::heading;
use crate::ui::image;
use crate::ui::link;
use crate::ui::text;

/// Pricing screen.
pub fn pricing_screen() -> Element {
	let title = heading(1, "Hey, this is free.");
	let intro = text(
		"Use monochange for your release planning. No paid plan to choose. No checkout to get through.",
	);
	let install = link("/install", "Install monochange");
	let book = link(BOOK_URL, "Read the book");
	let mark = image("/branding/mark.svg");
	let source_heading = heading(2, "Open source, too.");
	let source_copy = text(
		"Read the code, report an issue, or contribute an improvement. The CLI and application live in the same repository.",
	);
	let source = link(SOURCE_URL, "Visit monochange on GitHub");
	view! {
		<View class="flex flex-col gap-4">
			{title}
			{intro}
			{install}
			{book}
			{mark}
			{source_heading}
			{source_copy}
			{source}
		</View>
	}
}
