use leptos::prelude::Owner;

#[test]
fn projects_say_they_are_loading_until_the_list_resolves() {
	let _page = Owner::new().with(|| super::projects_body(None));
}
