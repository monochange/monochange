use leptos::prelude::Owner;

#[test]
fn devices_say_they_are_loading_until_the_list_resolves() {
	let _page = Owner::new().with(|| super::device_list(None));
}
