#![allow(clippy::disallowed_methods)]

use leptos::prelude::*;

use super::WorkspaceEmpty;

#[tokio::test]
async fn empty_workspace_offers_the_next_step_for_each_session() {
	let signed_out = crate::tests::render(|| view! { <WorkspaceEmpty signed_in=false /> }).await;
	assert!(signed_out.contains("Sign in with GitHub"));
	assert!(!signed_out.contains("Connect repositories"));

	let signed_in = crate::tests::render(|| view! { <WorkspaceEmpty signed_in=true /> }).await;
	assert!(signed_in.contains("Connect repositories"));
	assert!(!signed_in.contains("Sign in with GitHub"));
}
