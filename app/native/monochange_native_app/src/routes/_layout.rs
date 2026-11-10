//! Stack back control. The tab bar comes from `navigator()`, which the file
//! router shells around this layout.

use leptoskit::prelude::*;

/// Floor and Install are the native tab roots. Other screens push on the stack.
pub fn navigator() -> leptoskit_router::navigators::Tabs<crate::routes::Route> {
	use leptoskit_router::navigators::Tab;
	use leptoskit_router::navigators::Tabs;

	Tabs::new([
		Tab::root(crate::routes::Route::Index).title("Floor"),
		Tab::root(crate::routes::Route::Install).title("Install"),
	])
}

pub fn layout(children: Children, navigation: &crate::routes::Navigation) -> Element {
	let mut page = Vec::new();
	children.append_to(&mut page);
	let back = back_control(navigation);
	view! {
		<View class="flex min-h-screen flex-col bg-white">
			{back}
			<View class="flex flex-col gap-4 p-6">{page}</View>
		</View>
	}
}

fn back_control(navigation: &crate::routes::Navigation) -> Element {
	if !navigation.can_go_back() {
		return view! { <View /> };
	}

	let navigation = navigation.clone();
	view! {
		<Button
			class="rounded-lg px-4 py-2 font-semibold text-slate-900"
			on:press=move || {
				navigation.back();
			}
		>
			"Back"
		</Button>
	}
}
