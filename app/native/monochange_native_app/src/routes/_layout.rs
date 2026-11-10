//! Stack back control and the tab row. Published leptoskit 0.3 has no
//! Stack/Tabs `navigator()`; this is the in-memory router those guides describe.

use leptoskit::prelude::*;

pub fn layout(children: Children, navigation: &crate::routes::Navigation) -> Element {
	let mut page = Vec::new();
	children.append_to(&mut page);

	View()
		.class("flex min-h-screen flex-col bg-white")
		.child(back_control(navigation))
		.child(tabs(navigation))
		.child(View().class("flex flex-col gap-4 p-6").children(page))
}

fn back_control(navigation: &crate::routes::Navigation) -> Element {
	if !navigation.can_go_back() {
		return View();
	}

	let navigation = navigation.clone();
	Button("Back")
		.class("rounded-lg px-4 py-2 font-semibold text-slate-900")
		.on_press(move || {
			navigation.back();
		})
}

fn tabs(navigation: &crate::routes::Navigation) -> Element {
	View()
		.class("flex flex-row gap-4 p-4")
		.child(tab(navigation, crate::routes::Route::Index, "Floor"))
		.child(tab(navigation, crate::routes::Route::Install, "Install"))
}

fn tab(
	navigation: &crate::routes::Navigation,
	route: crate::routes::Route,
	label: &str,
) -> Element {
	navigation
		.link(route, label)
		.class("px-4 py-2 font-semibold text-slate-900")
		.child(Text(label))
}
