//! monochange projects: the shared product UI and server-function library.
//!
//! This crate holds the product pages' shared foundation — components,
//! server functions, the embedded feedback panel, markdown, and filters —
//! used by both the projects browser entry
//! ([`monochange_projects_web`], which owns the product route tree and
//! hydration) and the website backend, which composes the product surface
//! into `/projects` and `/p` SSR responses. It declares no browser entry of
//! its own: the `#[lazy_route]` wasm-split exports of the product route tree
//! live in the entry crate, so linking this library into the website client
//! cannot retain the product routes.
#![recursion_limit = "256"]
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
