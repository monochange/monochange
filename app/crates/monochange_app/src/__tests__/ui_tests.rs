use leptos::prelude::Owner;

#[test]
fn a_row_is_a_horizontal_view() {
	let _row = Owner::new().with(|| super::row("flex flex-row gap-4"));
}
