//! `/install` — the same public path as the website, as a native screen.

use leptoskit::prelude::*;

pub fn page() -> Element {
	View()
		.class("flex flex-col gap-4 p-6")
		.child(Text("Install monochange").class("text-2xl font-bold text-slate-900"))
		.child(Text(
			"cargo install monochange, then connect the GitHub App from the website.",
		))
}
