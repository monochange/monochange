//! The renderer boundary between generated file routes and the Leptos site.
//!
//! The pinned generator writes one `Route` enum over the files in this
//! directory and also an `impl Render` that calls each file's `page` and
//! decorates it with `leptoskit_native::Element`. That native tree cannot
//! carry this website's SSR resources, named forms, server functions, or
//! per-route wasm chunks, so the Leptos route tree still renders every screen.
//!
//! The website's runtime use of the generated module is the typed `Route`
//! value: [`crate::typed_routes`] matches request paths, derives book chapter
//! hrefs, expands the generated HTTP path list for SSR registration, and
//! validates consent returns from it. Each route file still exports the `page`
//! the generator requires; it returns this boundary marker, which keeps a
//! native host that renders the generated module safe and empty rather than
//! duplicating the Leptos screens in a second product tree.
//!
//! Moving a screen here is only appropriate when its native rendering no
//! longer needs SSR data or a lazy chunk; document that change before doing it.

use leptoskit_router::__private::Element;

/// An empty native page for a route whose real screen is a Leptos view.
pub fn deferred_to_leptos() -> Element {
	Element::text("")
}
