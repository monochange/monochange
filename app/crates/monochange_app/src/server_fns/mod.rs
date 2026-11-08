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
pub mod auth;
pub mod book;
pub mod embedded;
pub mod feedback;
pub mod installation;
pub mod organizations;
pub mod portal;

pub mod releases;

pub mod repos;
