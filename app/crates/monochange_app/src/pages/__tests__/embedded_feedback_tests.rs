#![allow(clippy::disallowed_methods)]
#![cfg(not(target_arch = "wasm32"))]

use monochange_app_feedback::planning::ImplementationPlan;
use monochange_app_feedback::planning::PlanningBrief;
use monochange_app_feedback::planning::PlanningState;

use super::*;
use crate::server_fns::embedded::IntegrationView;
use crate::server_fns::embedded::WorkerView;

#[tokio::test]
async fn integration_settings_and_complete_plan_review_render_as_forms() {
	let mut planning = PlanningState::default();
	let id = planning
		.queue(
			PlanningBrief {
				item: "fb-1".into(),
				version: "v1".into(),
				repository: "alice/web".into(),
				commit: "a".repeat(40),
				text: "CSV export".into(),
			},
			false,
			100,
		)
		.unwrap();
	planning
		.claim("worker", "digest", &["alice/web".into()], 101)
		.unwrap();
	let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../../fixtures/tests/feedback-widget/flow/plan.json");
	let plan: ImplementationPlan =
		serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
	planning
		.complete_plan(&id, "worker", "digest", plan, 102)
		.unwrap();
	let data = EmbeddedView {
		project_id: 1,
		integrations: vec![IntegrationView {
			id: "web".into(),
			name: "Customer app".into(),
			namespace: "accounts".into(),
			origins: vec!["https://example.com".into()],
			revoked: false,
		}],
		workers: vec![WorkerView {
			id: "worker".into(),
			name: "Local agent".into(),
			repositories: vec!["alice/web".into()],
			last_seen: "3 Nov 2026 12:00 UTC".into(),
			revoked: false,
		}],
		policy: planning.policy,
		jobs: planning.jobs,
		repositories: vec!["alice/web".into()],
		items: vec![("fb-1".into(), "Export CSV".into())],
		assistant_enabled: false,
	};

	let html = crate::tests::render(move ||view! {<EmbeddedContent data=data.clone() organization="alice".to_owned() project="pocketbook".to_owned() action=ServerAction::new() />}).await;
	// HTML lacks structural redaction selectors; nonces and compiled action hashes vary by build.
	insta::with_settings!({ filters => vec![(r#"nonce="[^"]*""#, r#"nonce="[nonce]""#), (r"/api/embedded_action[0-9]+", "/api/embedded_action[build-hash]")] }, {
		insta::assert_snapshot!(html.replace("><", ">\n<"));
	});
}

#[tokio::test]
async fn credentials_and_action_failures_have_accessible_status_messages() {
	let html=crate::tests::render(||view! {
  {action_notice(Some(Ok(EmbeddedReceipt {message:"Copy this credential now".into(),credential:Some("test-only-one-time-code".into())})))}
  {action_notice(Some(Err(server_fn::ServerFnError::new("Choose a connected repository"))))}
 }).await;
	insta::with_settings!({filters=>vec![(r#"nonce="[^"]*""#,r#"nonce="[nonce]""#), (r"/api/embedded_action[0-9]+", "/api/embedded_action[build-hash]")]},{insta::assert_snapshot!(html.replace("><",">\n<"));});
}

#[tokio::test]
async fn automatic_planning_explains_the_approval_requirement() {
	let data = EmbeddedView {
		project_id: 1,
		integrations: vec![],
		workers: vec![],
		policy: monochange_app_feedback::planning::PlanningPolicy {
			automatic_features: true,
			..Default::default()
		},
		jobs: vec![],
		repositories: vec!["alice/web".into()],
		items: vec![],
		assistant_enabled: true,
	};
	let html=crate::tests::render(move ||view! {<EmbeddedContent data=data.clone() organization="alice".to_owned() project="pocketbook".to_owned() action=ServerAction::new() />}).await;
	insta::with_settings!({filters=>vec![(r#"nonce="[^"]*""#,r#"nonce="[nonce]""#), (r"/api/embedded_action[0-9]+", "/api/embedded_action[build-hash]")]},{insta::assert_snapshot!(html.replace("><",">\n<"));});
}
