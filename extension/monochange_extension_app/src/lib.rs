//! monochange browser extension app surfaces.
//!
//! Default builds compile the popup model and theme/Tailwind helpers without
//! leptoskit (so `cargo test` stays light). `leptoskit build --platform
//! extension` enables the `extension` feature, which mounts the popup,
//! options page, and background worker through `leptoskit::extension::entry!`.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

pub mod chrome;
pub mod popup;
pub mod session;

#[cfg(feature = "extension")]
mod extension;

#[cfg(feature = "extension")]
leptoskit::extension::entry!(main: extension::main, background: extension::background);

/// Default API origin when the extension manifest omits `server`.
pub const DEFAULT_SERVER: &str = match option_env!("MONOCHANGE_SERVER") {
	Some(server) => server,
	None => "https://monochange.dev",
};
#[cfg(feature = "extension")]
mod identity;
