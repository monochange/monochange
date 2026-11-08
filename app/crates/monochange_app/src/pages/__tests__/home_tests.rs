//! Landing page SSR coverage for the Release floor redesign.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::auth::SessionUser;

fn sample_user() -> SessionUser {
	SessionUser {
		github_id: 101,
		github_login: "alice".to_owned(),
		github_avatar_url: None,
		plan_tier: "free".to_owned(),
	}
}

#[test]
fn workspace_href_prefers_dashboard_when_signed_in() {
	assert_eq!(
		workspace_href_for(Some(Ok(Some(sample_user())))),
		"/dashboard"
	);
	assert_eq!(workspace_href_for(Some(Ok(None))), "/login");
	assert_eq!(
		workspace_href_for(Some(Err(server_fn::ServerFnError::new("missing")))),
		"/login"
	);
	assert_eq!(workspace_href_for(None), "/login");
}

#[test]
fn release_manifest_helpers_cover_both_strategies() {
	assert!(strategy_button_class(true).contains("bg-brand-100"));
	assert!(strategy_button_class(false).contains("text-mute"));
	assert_eq!(core_versions(false), ("0.8.1", "0.9.0"));
	assert_eq!(core_versions(true), ("1.4.2", "1.5.0"));
	assert_eq!(mobile_versions(false), ("2.1.0", "2.1.1"));
	assert_eq!(mobile_versions(true), ("1.4.2", "1.5.0"));
	assert!(strategy_copy(false).contains("own version"));
	assert!(strategy_copy(true).contains("shared release identity"));

	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	runtime.block_on(async {
		Owner::new().with(|| {
			let (shared, set_shared) = signal(false);
			select_shared_version(set_shared);
			assert!(shared.get());
			select_individual_versions(set_shared);
			assert!(!shared.get());
		});
	});
}

#[tokio::test]
async fn home_page_renders_release_floor_sections() {
	let html = crate::tests::render(|| view! { <HomePage /> }).await;

	assert!(html.contains("Ship every package from one floor."));
	assert!(html.contains("The product floor"));
	assert!(html.contains("One plan across packages"));
	assert!(html.contains("Different ecosystems."));
	assert!(html.contains("Know what ships."));
	assert!(html.contains("Make the next release less complicated."));
	assert!(html.contains("Your next release"));
	assert!(html.contains("@acme/ui"));
	assert!(html.contains("acme-core"));
	assert!(html.contains("acme-mobile"));
	assert!(html.contains("Individual versions"));
	assert!(html.contains("Shared version"));
	assert!(html.contains("href=\"/login\"") || html.contains("href=\"/dashboard\""));
}
