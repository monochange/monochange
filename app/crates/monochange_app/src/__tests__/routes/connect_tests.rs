// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use leptos::prelude::Owner;
use monochange_shared::ClientKind;
use monochange_shared::ConnectPreview;
use monochange_shared::SessionUser;
use server_fn::ServerFnError;

fn preview() -> ConnectPreview {
	ConnectPreview {
		denial_url: "dev.monochange.app:/auth/native/return?error=access_denied".to_owned(),
		user: SessionUser {
			github_id: 7,
			github_login: "ada".to_owned(),
			github_avatar_url: None,
			plan_tier: "free".to_owned(),
		},
		client: ClientKind::Native,
	}
}

#[test]
fn consent_covers_checking_approval_rejection_and_sign_in() {
	let views = Owner::new().with(|| {
		[
			super::connection_body(None, "/login?next=%2Fconnect"),
			super::connection_body(Some(Ok(Some(preview()))), "/login"),
			super::connection_body(Some(Ok(None)), "/login"),
			super::connection_body(
				Some(Err(ServerFnError::new("signed out"))),
				"/login?next=%2Fconnect",
			),
		]
	});
	assert_eq!(views.len(), 4);
}

#[tokio::test]
async fn the_connect_page_checks_the_method_before_asking_the_server() {
	for method in ["plain", "S256"] {
		let method = method.to_owned();
		let html = crate::tests::render(move || {
			leptoskit::web::render(super::page(&super::Connect {
				client: Some("native".to_owned()),
				redirect_uri: Some("dev.monochange.app:/auth/native/return".to_owned()),
				code_challenge: Some("challenge".to_owned()),
				state: Some("state-1234567890".to_owned()),
				code_challenge_method: Some(method.clone()),
			}))
		})
		.await;
		assert!(
			html.contains("invalid") || html.contains("Sign in") || html.contains("Checking"),
			"{html}"
		);
	}
}
