//! Shared monochange RPC payloads for web and native leptoskit clients.
//!
//! Keep this crate free of platform hosts so the web SSR port can path-dep it
//! without pulling Android/iOS toolchains. Operation bodies live behind the
//! `server` feature; the `client` feature exposes the generated argument
//! structs for `leptoskit_rpc::client::call`.

#![forbid(unsafe_code)]

mod types;

pub use types::AuthHandshake;
pub use types::ProjectSummary;
pub use types::SessionUser;

#[cfg(any(feature = "server", feature = "client"))]
mod ops;

#[cfg(any(feature = "server", feature = "client"))]
pub use ops::ListProjects;
#[cfg(any(feature = "server", feature = "client"))]
pub use ops::SessionProbe;
#[cfg(feature = "server")]
pub use ops::demo_projects;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
