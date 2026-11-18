use super::*;

#[test]
fn home_constructs_a_projects_screen_with_sign_in_and_management_actions() {
	fn text(element: &Element, output: &mut Vec<String>) {
		if let Some(value) = &element.props.text {
			output.push(value.clone());
		}
		for child in &element.children {
			text(child, output);
		}
	}
	let owner = reactive_graph::owner::Owner::new();
	let model = owner.with(HomeModel::restore);
	model.state.set(State::SignedOut);
	let home = owner.with(|| view! { <Home model=model /> });
	let mut labels = Vec::new();
	text(&home, &mut labels);
	assert!(labels.iter().any(|label| label == "monochange projects"));
	assert!(labels.iter().any(|label| label == "Sign in"));
	assert!(
		labels
			.iter()
			.any(|label| label == "Manage projects on the website")
	);
	assert!(!labels.iter().any(|label| label == "Sign out"));
	model.state.set(State::Failed("offline".into()));
	let updated = owner.with(|| view! { <Home model=model /> });
	labels.clear();
	text(&updated, &mut labels);
	assert!(labels.iter().any(|label| label.contains("offline")));
}

fn signed_in_model() -> HomeModel {
	let model = HomeModel::restore();
	model.credential.set(Some(ClientSession {
		id: "device".into(),
		access_token: "mcd_dummy".into(),
		expires_at: 123,
		user: monochange_shared::SessionUser {
			github_id: 101,
			github_login: "alice".into(),
			github_avatar_url: None,
			plan_tier: "free".into(),
		},
	}));
	model
}

#[tokio::test]
async fn offline_project_error_keeps_the_device_credential() {
	let owner = reactive_graph::owner::Owner::new();
	let model = owner.with(signed_in_model);
	handle_error(
		model.credential,
		model.state,
		monochange_shared::ApiError::Transport("offline".into()),
	)
	.await;
	assert_eq!(model.credential.get().unwrap().access_token, "mcd_dummy");
	assert!(matches!(model.state.get(), State::Failed(message) if message.contains("offline")));
}

#[tokio::test]
async fn failed_secure_storage_cleanup_is_visible_after_an_unauthorized_response() {
	let owner = reactive_graph::owner::Owner::new();
	let model = owner.with(signed_in_model);
	handle_error(
		model.credential,
		model.state,
		monochange_shared::ApiError::Status {
			status: 401,
			message: "revoked".into(),
		},
	)
	.await;
	assert!(matches!(model.state.get(), State::Failed(message) if message.contains("storage")));
	assert!(model.credential.get().is_some());
}
