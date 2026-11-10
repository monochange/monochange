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
	View()
		.class("flex flex-col gap-4")
		.child(heading(1, "Hey, this is free."))
		.child(text(
			"Use monochange for your release planning. No paid plan to choose. No checkout to get through.",
		))
		.child(link("/install", "Install monochange"))
		.child(link(BOOK_URL, "Read the book"))
		.child(image("/branding/mark.svg"))
		.child(heading(2, "Open source, too."))
		.child(text(
			"Read the code, report an issue, or contribute an improvement. The CLI and application live in the same repository.",
		))
		.child(link(SOURCE_URL, "Visit monochange on GitHub"))
}
