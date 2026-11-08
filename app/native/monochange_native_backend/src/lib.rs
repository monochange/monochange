//! Server-side entry points for the native scaffold.
//!
//! Linking this crate pulls in `monochange_rpc` with the `server` feature so
//! `session_probe` / `list_projects` register with leptoskit_rpc. Mount the
//! resulting router from the `monochange_native` binary (or the future web
//! port) with:
//!
//! ```ignore
//! use leptoskit::rpc as rpc;
//! let rpc = rpc::server::Server::new(db);
//! let app = axum::Router::new().merge(rpc.router());
//! ```
//!
//! Scaffold handlers return anonymous handshake + demo projects until the
//! shared auth/DB adapter lands.

#![forbid(unsafe_code)]

pub use monochange_rpc::AuthHandshake;
pub use monochange_rpc::ProjectSummary;
pub use monochange_rpc::SessionUser;

/// Ensures RPC operation bodies are linked into the server binary.
pub fn register_rpc_ops() {
	// Touching the module is enough for leptoskit_rpc's inventory registration
	// when the `server` feature is enabled on `monochange_rpc`.
	let _ = std::any::type_name::<monochange_rpc::SessionProbe>();
	let _ = std::any::type_name::<monochange_rpc::ListProjects>();
}
