//! Shared wire types for monochange product surfaces.
//!
//! The website WASM client, and future browser-extension / native guests, should
//! depend on this crate for session and dashboard DTOs. Keep Axum, SQLite, OAuth
//! cookie builders, and GitHub App signing out of this crate.
//!
//! Product RPCs live as Leptos `#[server]` functions (migrating to
//! `leptoskit_rpc`). Machine webhooks, hosted release OIDC, and the embed SDK
//! stay on thin Axum adapters — see `monochange_backend`.

pub mod auth;
pub mod book;
pub mod installation;
pub mod organizations;
pub mod releases;
pub mod repos;

pub use auth::SessionUser;
pub use book::BookChapter;
pub use book::BookPage;
pub use book::ChapterLink;
pub use installation::PermissionUpdate;
pub use installation::RepositoryConnectionStatus;
pub use organizations::OrganizationSummary;
pub use organizations::ProjectDetails;
pub use organizations::ProjectRepositoryView;
pub use organizations::ProjectSummary;
pub use organizations::feedback_console_path;
pub use organizations::organization_path;
pub use organizations::project_path;
pub use releases::WebsiteEntry;
pub use releases::WebsiteRelease;
pub use releases::WebsiteSection;
pub use repos::RepoInfo;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
