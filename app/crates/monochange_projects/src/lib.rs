//! monochange projects: workspace UI and its authorized product operations.
#![recursion_limit = "256"]
pub mod components;
pub mod markdown;
pub mod pages;
pub mod projects;
pub mod repository_filter;
pub mod server_fns;
#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
