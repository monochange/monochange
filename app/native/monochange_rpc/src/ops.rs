//! leptoskit_rpc operations for the auth handshake and project list.
//!
//! Bodies compile only with the `server` feature. Guests and the browser link
//! the `client` feature and call the generated argument structs.

use leptoskit::rpc;
use leptoskit::rpc::api;

use crate::types::AuthHandshake;
use crate::types::ProjectSummary;

/// Probes the caller's session without requiring a full OAuth round-trip.
///
/// Scaffold behaviour: returns an anonymous handshake. Wire this to the
/// monochange JWT cookie / bearer token once the web port mounts the same
/// router beside GitHub OAuth.
///
/// Published RPC name remains `auth_handshake` for clients; the Rust
/// argument struct is `SessionProbe` so it does not collide with
/// [`AuthHandshake`].
#[api(tag = "auth", name = "auth_handshake")]
#[rpc::query(crate = "leptoskit::rpc")]
pub async fn session_probe(_cx: &rpc::QueryCx) -> rpc::Result<AuthHandshake> {
	Ok(AuthHandshake::anonymous())
}

/// Lists projects the signed-in caller can manage.
///
/// Scaffold behaviour: returns a demo row so native shells can exercise
/// list rendering before the DB adapter lands.
#[api(tag = "projects", name = "list_projects")]
#[rpc::query(crate = "leptoskit::rpc")]
pub async fn list_projects(cx: &rpc::QueryCx) -> rpc::Result<Vec<ProjectSummary>> {
	let _ = cx;
	Ok(demo_projects())
}

/// Demo projects used until `monochange_app_db` is wired through QueryCx.
#[cfg(feature = "server")]
#[must_use]
pub fn demo_projects() -> Vec<ProjectSummary> {
	vec![ProjectSummary {
		organization: "monochange".to_owned(),
		slug: "workspace".to_owned(),
		name: "Workspace".to_owned(),
		description: "Group repos under one release identity.".to_owned(),
		repository_count: 0,
	}]
}
