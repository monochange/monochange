//! Server functions — product UI RPCs that run on the server.
//!
//! These use Leptos `#[server]` (and, as the leptoskit port lands,
//! `leptoskit_rpc` `#[api]`) so the same signatures compile to client-side
//! fetch calls and server-side implementations.
//!
//! Wire DTOs that extension/native guests need live in `monochange_shared`
//! and are re-exported from these modules so existing imports keep working.
//! Machine webhooks / OIDC / embed SDK routes stay on Axum via
//! `monochange_backend::machine_api_router`.

pub mod ai;
pub use monochange_projects::server_fns::auth;
pub mod book;
pub use monochange_projects::server_fns::embedded;
pub use monochange_projects::server_fns::feedback;
pub mod installation;
pub use monochange_projects::server_fns::organizations;
pub use monochange_projects::server_fns::portal;

pub mod releases;

pub use monochange_projects::server_fns::repos;
pub mod session;

#[cfg(not(target_arch = "wasm32"))]
pub use monochange_projects::server_fns::principal;
