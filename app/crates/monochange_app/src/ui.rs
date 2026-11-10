//! Small leptoskit views for site screens.
//!
//! Classes are limited to utilities the leptoskit Tailwind parser accepts.
//! Custom `@theme` names such as `bg-brand-700` panic at render time.

use leptoskit::prelude::*;

/// The page column used by marketing and workspace screens.
pub const PAGE: &str = "flex min-h-screen flex-col gap-6 p-6";

/// A vertical stack.
pub fn column(class: &str) -> Element {
	let class = class.to_owned();
	view! { <View class=class /> }
}

/// A horizontal row.
pub fn row(class: &str) -> Element {
	let class = class.to_owned();
	view! { <View class=class /> }
}

/// Appends every child, skipping nothing.
pub fn with_children(mut element: Element, children: impl IntoIterator<Item = Element>) -> Element {
	for child in children {
		element = element.child(child);
	}
	element
}

/// Body text.
pub fn text(value: impl Into<String>) -> Element {
	let value = value.into();
	view! { <Text>{value}</Text> }
}

/// Body text with utilities.
pub fn text_class(class: &str, value: impl Into<String>) -> Element {
	let class = class.to_owned();
	let value = value.into();
	view! { <Text class=class>{value}</Text> }
}

/// A heading. Levels above 6 still render as headings in the kit.
pub fn heading(level: u8, value: impl Into<String>) -> Element {
	let value = value.into();
	let level = u32::from(level);
	view! { <Heading aria_level=level class="text-2xl font-bold text-slate-900">{value}</Heading> }
}

/// A real link. The label is both the accessible name and the visible text.
pub fn link(href: impl Into<String>, label: &str) -> Element {
	let href = href.into();
	let label = label.to_owned();
	let visible = label.clone();
	view! {
		<Pressable aria_label=label href=href class="px-4 py-2 font-semibold text-teal-700">
			<Text>{visible}</Text>
		</Pressable>
	}
}

/// A button that does not navigate.
pub fn button(label: &str) -> Element {
	let label = label.to_owned();
	view! { <Button class="rounded-lg bg-teal-700 px-4 py-2 font-bold text-white">{label}</Button> }
}

/// An image. Empty alt text marks it decorative.
pub fn image(source: impl Into<String>) -> Element {
	let source = source.into();
	view! { <Image source=source aria_label="" /> }
}
