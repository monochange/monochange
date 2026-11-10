//! monochange projects: workspace UI and its authorized product operations.
//!
//! This crate is the projects browser entry (`cdylib` + `hydrate`) as well
//! as the product page and server-function library the website backend
//! composes for `/projects` and `/p` SSR responses.
#![recursion_limit = "256"]
pub mod app;
pub mod components;
pub mod error;
pub mod markdown;
pub mod pages;
pub mod projects;
pub mod repository_filter;
pub mod server_fns;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

/// Hydrate the projects app on the client (WASM only).
#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub use app::hydrate;
