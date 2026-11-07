//! Server-only mount surface for the monochange website.
//!
//! ## Product RPC (Class A)
//!
//! UI-facing operations stay as Leptos `#[server]` functions in
//! `monochange_app::server_fns` while the leptoskit SSR + `leptoskit_rpc`
//! port lands. Those modules will move here (or be re-exported from here)
//! once the app links `leptoskit_rpc` 0.3 and annotates them with `#[api]`.
//! Wire DTOs for those RPCs live in `monochange_shared`.
//!
//! ## Machine HTTP (Class B)
//!
//! GitHub App webhooks, hosted release commit/request endpoints (OIDC /
//! `MONOCHANGE_TOKEN`), public CLI task GETs, and the embed SDK
//! (`/api/v1/feedback/...`) remain thin Axum adapters. Mount them with
//! [`machine_api_router`], which forwards to
//! [`monochange_app_api::api_router`]. Do not rewrite these as Leptos server
//! functions: native guests, CI OIDC callers, and the embed iframe need
//! stable plain-HTTP contracts.
//!
//! ## Auth cookies (preserve)
//!
//! Browser sessions keep `__Host-monochange_session` (7-day JWT) and
//! `__Host-monochange_oauth` (short-lived OAuth state). Cookie builders stay
//! in `monochange_app_api::oauth`; this crate only mounts the routers that
//! honor them.

use axum::Router;
use monochange_app_api::AppState;
pub use monochange_app_api::AppState as BackendAppState;
pub use monochange_app_api::api_router;

/// Mount Class B machine HTTP routes (webhooks, hosted release, tasks, embed).
///
/// Merge the returned router beside Leptos SSR / server-fn routes in the
/// website binary. Rate limits and body limits belong on the outer app
/// router, as they do today.
pub fn machine_api_router(state: AppState) -> Router<()> {
	api_router(state)
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
