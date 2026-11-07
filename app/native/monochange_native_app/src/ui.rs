//! Shared native controls styled with Release floor tokens.

use leptoskit::prelude::*;

use crate::theme;

#[component]
pub fn Title(text: impl Into<String>) -> Element {
	view! {
		<Heading aria_level=1 class=format!("text-2xl font-extrabold tracking-tight {}", theme::INK)>
			{text.into()}
		</Heading>
	}
}

#[component]
pub fn Section(text: impl Into<String>) -> Element {
	view! {
		<Heading aria_level=2 class=format!("text-lg font-bold {}", theme::INK)>
			{text.into()}
		</Heading>
	}
}

#[component]
pub fn Panel(children: Children) -> Element {
	view! {
		<View class=theme::SURFACE>
			{children}
		</View>
	}
}

#[component]
pub fn Primary(label: &str, enabled: bool, on_press: impl Fn() + Send + Sync + 'static) -> Element {
	view! {
		<Button
			enabled=enabled
			class=theme::BUTTON_PRIMARY
			class=if enabled { "" } else { "opacity-50" }
			on:press=on_press
		>
			{label}
		</Button>
	}
}

#[component]
pub fn Secondary(label: &str, on_press: impl Fn() + Send + Sync + 'static) -> Element {
	view! {
		<Button class=theme::BUTTON_SECONDARY on:press=on_press>
			{label}
		</Button>
	}
}

#[component]
pub fn Quiet(text: impl Into<String>) -> Element {
	view! { <Text class=theme::MUTED>{text.into()}</Text> }
}

#[component]
pub fn Problem(message: &str) -> Element {
	view! {
		<Alert class="rounded-[12px] bg-red-100 px-3 py-2 text-red-900 dark:bg-red-950 dark:text-red-100">
			{message}
		</Alert>
	}
}

#[component]
pub fn Loading(what: &str) -> Element {
	view! { <Status class=theme::MUTED>{format!("Loading {what}…")}</Status> }
}
