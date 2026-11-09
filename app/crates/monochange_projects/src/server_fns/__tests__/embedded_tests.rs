#![allow(clippy::disallowed_methods)]
#![cfg(not(target_arch = "wasm32"))]

use leptos::reactive::computed::ScopedFuture;

use super::*;
use crate::tests::feedback::context;
use crate::tests::feedback::state;

#[tokio::test]
async fn only_project_maintainers_can_manage_clients_workers_and_plans() {
	let state = state().await;

	for user in [None, Some(2), Some(3)] {
		let (owner, _) = context(&state, user);
		assert!(
			owner
				.with(|| ScopedFuture::new(embedded_console("alice".into(), "pocketbook".into())))
				.await
				.is_err()
		);
		assert!(
			owner
				.with(|| {
					ScopedFuture::new(embedded_action(
						"alice".into(),
						"pocketbook".into(),
						"policy".into(),
						None,
						None,
						None,
						None,
						None,
						None,
						None,
						false,
						None,
					))
				})
				.await
				.is_err()
		);
	}

	let (owner, _) = context(&state, Some(1));
	let registered = owner
		.with(|| {
			ScopedFuture::new(embedded_action(
				"alice".into(),
				"pocketbook".into(),
				"register".into(),
				None,
				Some("Web".into()),
				Some("accounts".into()),
				Some("https://example.com".into()),
				Some("both".into()),
				None,
				None,
				false,
				None,
			))
		})
		.await
		.unwrap();
	let plaintext = registered.credential.unwrap();

	let view = owner
		.with(|| ScopedFuture::new(embedded_console("alice".into(), "pocketbook".into())))
		.await
		.unwrap();
	assert_eq!(view.integrations.len(), 1);
	assert!(!serde_json::to_string(&view).unwrap().contains(&plaintext));
	owner
		.with(|| {
			ScopedFuture::new(embedded_action(
				"alice".into(),
				"pocketbook".into(),
				"policy".into(),
				None,
				None,
				None,
				None,
				None,
				Some("alice/web".into()),
				None,
				true,
				Some(4),
			))
		})
		.await
		.unwrap();
	let view = owner
		.with(|| ScopedFuture::new(embedded_console("alice".into(), "pocketbook".into())))
		.await
		.unwrap();
	assert!(view.policy.automatic_features);
	assert_eq!(view.policy.daily_limit, 4);
	let paired = owner
		.with(|| {
			ScopedFuture::new(embedded_action(
				"alice".into(),
				"pocketbook".into(),
				"pair_worker".into(),
				None,
				Some("Local".into()),
				None,
				None,
				None,
				Some("alice/web".into()),
				None,
				false,
				None,
			))
		})
		.await
		.unwrap();

	assert!(paired.credential.unwrap().starts_with("mcfp.1."));
}

async fn action(
	state: &std::sync::Arc<monochange_app_api::AppState>,
	command: &str,
	id: Option<&str>,
	surface: Option<&str>,
) -> Result<EmbeddedReceipt, server_fn::ServerFnError> {
	let (owner, _) = context(state, Some(1));
	owner
		.with(|| {
			ScopedFuture::new(embedded_action(
				"alice".into(),
				"pocketbook".into(),
				command.into(),
				id.map(str::to_owned),
				Some("App".into()),
				Some("accounts".into()),
				Some("https://example.com".into()),
				surface.map(str::to_owned),
				Some("alice/web".into()),
				Some("Maintainer verified the worker stopped".into()),
				false,
				Some(4),
			))
		})
		.await
}

#[tokio::test]
async fn integration_actions_enforce_form_choices_and_revoke_credentials() {
	let state = state().await;
	assert!(required(None).is_err());
	assert!(required(Some(" ")).is_err());

	for surface in ["web", "mobile"] {
		assert!(
			action(&state, "register", None, Some(surface))
				.await
				.unwrap()
				.credential
				.is_some()
		);
	}

	assert!(
		action(&state, "register", None, Some("invalid"))
			.await
			.is_err()
	);
	assert!(action(&state, "unknown", None, None).await.is_err());
	let (owner, _) = context(&state, Some(1));

	let data = owner
		.with(|| ScopedFuture::new(embedded_console("alice".into(), "pocketbook".into())))
		.await
		.unwrap();
	let id = &data.integrations[0].id;
	assert!(
		action(&state, "rotate", Some(id), None)
			.await
			.unwrap()
			.credential
			.is_some()
	);
	assert!(
		action(&state, "revoke_integration", Some(id), None)
			.await
			.unwrap()
			.credential
			.is_none()
	);
	assert!(
		action(&state, "revoke_worker", Some("missing"), None)
			.await
			.is_err()
	);
	assert!(action(&state, "plan", Some("missing"), None).await.is_err());
	assert!(
		action(&state, "approve", Some("missing"), None)
			.await
			.is_err()
	);
}

#[tokio::test]
async fn plan_review_cancel_confirm_and_retry_actions_use_the_current_brief() {
	use monochange_app_api::feedback;
	use monochange_app_feedback::planning::ImplementationPlan;
	use monochange_app_feedback::planning::JobStatus;
	use monochange_app_feedback::planning::PlanningBrief;
	let state = state().await;
	let item = crate::tests::feedback::submit(
		&state,
		"reporter",
		monochange_app_feedback::FeedbackKind::FeatureRequest,
		"Export CSV",
	)
	.await;
	let public = feedback::public_project(&state, "alice", "pocketbook")
		.await
		.unwrap()
		.unwrap();
	let scope = &public.scope;
	let plan = ImplementationPlan {
		summary: "Export CSV".into(),
		acceptance: vec!["Includes totals".into()],
		changes: vec!["Add serializer".into()],
		paths: vec!["src/**".into()],
		checks: vec!["Run tests".into()],
		risks: vec![],
	};

	for command in ["reject", "changes"] {
		let id = feedback::update(&state, scope, |service| {
			let version = monochange_app_api::embedded_api::admin::brief_version(
				service,
				&item,
				"alice/web",
				&"a".repeat(40),
			)?;
			let id = service.planning_mut().queue(
				PlanningBrief {
					item: item.clone(),
					version,
					repository: "alice/web".into(),
					commit: "a".repeat(40),
					text: "Export CSV".into(),
				},
				false,
				1,
			)?;
			service
				.planning_mut()
				.claim("worker", "lease", &["alice/web".into()], 2)
				.unwrap();
			service
				.planning_mut()
				.complete_plan(&id, "worker", "lease", plan.clone(), 3)?;
			Ok(id)
		})
		.await
		.unwrap();
		action(&state, command, Some(&id), None).await.unwrap();

		// Use a different request revision for the next review, preserving immutable history.
		feedback::update(&state, scope, |service| {
			service.edit_summary(&item, "alice", command)?;
			Ok(())
		})
		.await
		.unwrap();
	}

	let id = feedback::update(&state, scope, |service| {
		let version = monochange_app_api::embedded_api::admin::brief_version(
			service,
			&item,
			"alice/web",
			&"a".repeat(40),
		)?;
		let id = service.planning_mut().queue(
			PlanningBrief {
				item: item.clone(),
				version,
				repository: "alice/web".into(),
				commit: "a".repeat(40),
				text: "Export CSV".into(),
			},
			false,
			10,
		)?;
		service
			.planning_mut()
			.claim("worker", "lease", &["alice/web".into()], 11)
			.unwrap();
		Ok(id)
	})
	.await
	.unwrap();
	action(&state, "cancel", Some(&id), None).await.unwrap();
	action(&state, "confirm_stopped", Some(&id), None)
		.await
		.unwrap();
	action(&state, "retry", Some(&id), None).await.unwrap();
	let loaded = feedback::load(&state, scope).await.unwrap();
	assert_eq!(
		loaded.service.planning().jobs.last().unwrap().status,
		JobStatus::Queued
	);
	assert!(action(&state, "cancel", None, None).await.is_err());
}

#[tokio::test]
async fn worker_console_omits_credentials_and_lease_secrets() {
	let state = state().await;
	let public = monochange_app_api::feedback::public_project(&state, "alice", "pocketbook")
		.await
		.unwrap()
		.unwrap();
	monochange_app_api::feedback::update(&state, &public.scope, |service| {
		service
			.embedded_mut()
			.workers
			.push(monochange_app_feedback::embedded::Worker {
				id: "worker".into(),
				name: "Self hosted".into(),
				credential_hash: "private-worker-digest".into(),
				repositories: vec!["alice/web".into()],
				revoked: false,
				last_seen: i64::MAX,
			});
		let id = service.planning_mut().queue(
			monochange_app_feedback::planning::PlanningBrief {
				item: "fb-1".into(),
				version: "v1".into(),
				repository: "alice/web".into(),
				commit: "a".repeat(40),
				text: "Export CSV".into(),
			},
			false,
			1,
		)?;
		service
			.planning_mut()
			.claim("worker", "private-lease-digest", &["alice/web".into()], 2)
			.unwrap();
		assert_eq!(id, "job-1");
		Ok(())
	})
	.await
	.unwrap();
	let (owner, _) = context(&state, Some(1));
	let data = owner
		.with(|| ScopedFuture::new(embedded_console("alice".into(), "pocketbook".into())))
		.await
		.unwrap();
	let json = serde_json::to_string(&data).unwrap();
	assert!(!json.contains("private-worker-digest"));
	assert!(!json.contains("private-lease-digest"));

	assert_eq!(data.workers[0].last_seen, "");
	action(&state, "revoke_worker", Some("worker"), None)
		.await
		.unwrap();
	let loaded = monochange_app_api::feedback::load(&state, &public.scope)
		.await
		.unwrap();
	assert!(loaded.service.embedded().workers[0].revoked);
}

#[tokio::test]
async fn invalid_budgets_and_stale_recovery_commands_are_rejected() {
	let state = state().await;
	let (owner, _) = context(&state, Some(1));
	assert!(
		owner
			.with(|| {
				ScopedFuture::new(embedded_action(
					"alice".into(),
					"pocketbook".into(),
					"policy".into(),
					None,
					None,
					None,
					None,
					None,
					None,
					None,
					false,
					Some(0),
				))
			})
			.await
			.is_err()
	);
	let item = crate::tests::feedback::submit(
		&state,
		"reporter",
		monochange_app_feedback::FeedbackKind::FeatureRequest,
		"Export CSV",
	)
	.await;
	let public = monochange_app_api::feedback::public_project(&state, "alice", "pocketbook")
		.await
		.unwrap()
		.unwrap();
	let id = monochange_app_api::feedback::update(&state, &public.scope, |service| {
		let version = monochange_app_api::embedded_api::admin::brief_version(
			service,
			&item,
			"alice/web",
			&"a".repeat(40),
		)?;
		service
			.planning_mut()
			.queue(
				monochange_app_feedback::planning::PlanningBrief {
					item: item.clone(),
					version,
					repository: "alice/web".into(),
					commit: "a".repeat(40),
					text: "Export CSV".into(),
				},
				false,
				1,
			)
			.map_err(Into::into)
	})
	.await
	.unwrap();
	assert!(action(&state, "retry", Some(&id), None).await.is_err());
	assert!(
		action(&state, "confirm_stopped", Some(&id), None)
			.await
			.is_err()
	);

	monochange_app_api::feedback::update(&state, &public.scope, |service| {
		service.edit_summary(&item, "alice", "Changed requirement")?;
		Ok(())
	})
	.await
	.unwrap();
	assert!(action(&state, "retry", Some(&id), None).await.is_err());
}
