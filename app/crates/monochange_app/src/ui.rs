//! Small leptoskit builders for site screens.
//!
//! Classes are limited to utilities the leptoskit 0.3 Tailwind parser accepts.
//! Custom `@theme` names such as `bg-brand-700` panic at render time.

use leptoskit::prelude::*;

/// The page column used by marketing and workspace screens.
pub const PAGE: &str = "flex min-h-screen flex-col gap-6 p-6";

/// A vertical stack.
pub fn column(class: &str) -> Element {
	View().class(class)
}

/// A horizontal row.
pub fn row(class: &str) -> Element {
	View().class(class)
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
	Text(value.into())
}

/// Body text with utilities.
pub fn text_class(class: &str, value: impl Into<String>) -> Element {
	Text(value.into()).class(class)
}

/// A heading. Levels above 6 still render as headings in the kit.
pub fn heading(level: u8, value: impl Into<String>) -> Element {
	Heading(level, value.into()).class("text-2xl font-bold text-slate-900")
}

/// A real link. The label is both the accessible name and the visible text.
pub fn link(href: impl Into<String>, label: &str) -> Element {
	let label = label.to_owned();
	Pressable(label.clone())
		.href(href.into())
		.class("px-4 py-2 font-semibold text-teal-700")
		.child(Text(label))
}

/// A button that does not navigate.
pub fn button(label: &str) -> Element {
	Button(label).class("rounded-lg bg-teal-700 px-4 py-2 font-bold text-white")
}

/// An image. Empty alt text marks it decorative.
pub fn image(source: impl Into<String>) -> Element {
	Image(source.into()).aria_label("")
}
