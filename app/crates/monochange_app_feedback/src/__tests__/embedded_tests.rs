use super::*;

fn integration() -> Integration {
	Integration {
		id: "app".into(),
		name: "Website".into(),
		namespace: "accounts".into(),
		origins: vec!["https://example.com".into()],
		surfaces: vec![ClientSurface::Web, ClientSurface::Mobile],
		credential_hash: "backend".into(),
		generation: 0,
		revoked: false,
	}
}

fn session() -> FeedbackSession {
	FeedbackSession {
		token_hash: "token".into(),
		integration: "app".into(),
		subject: "user".into(),
		surface: ClientSurface::Web,
		generation: 0,
		expires_at: 600,
	}
}

#[test]
fn sessions_expire_and_credential_rotation_revokes_them() {
	let mut state = EmbeddedState::default();
	state.register(integration(), "salt").unwrap();
	state.issue_session(session(), 0).unwrap();
	assert_eq!(state.session("token", 1).unwrap().subject, "user");
	assert!(state.session("token", 600).is_err());
	state.rotate("app", "next", false).unwrap();
	assert!(state.session("token", 1).is_err());
	assert!(state.issue_session(session(), 1).is_err());
}

#[test]
fn quotas_and_submission_keys_are_durable_and_idempotent() {
	let mut state = EmbeddedState::default();
	state.quota("user:submit", 1, 60, 1).unwrap();
	assert_eq!(
		state.quota("user:submit", 1, 60, 2),
		Err(EmbeddedError::RateLimited)
	);
	state.quota("user:submit", 1, 60, 60).unwrap();
	state
		.remember_submission(
			SubmissionKey {
				subject: "u".into(),
				key: "key".into(),
				fingerprint: "payload".into(),
				item: "fb-1".into(),
				expires_at: 100,
			},
			1,
		)
		.unwrap();
	assert_eq!(
		state.submitted("u", "key", "payload", 2).unwrap(),
		Some("fb-1".into())
	);
	assert!(state.submitted("u", "key", "changed", 2).is_err());
	assert_eq!(state.submitted("other", "key", "payload", 2).unwrap(), None);
}

#[test]
fn private_conversations_are_subject_and_integration_scoped() {
	let mut state = EmbeddedState::default();
	state
		.create_conversation(
			Conversation {
				id: "draft".into(),
				subject: "user".into(),
				integration: "app".into(),
				kind: FeedbackKind::FeatureRequest,
				context: None,
				messages: vec![ChatMessage {
					role: ChatRole::User,
					body: "CSV export".into(),
				}],
				version: 0,
				item: None,
				expires_at: 100,
			},
			1,
		)
		.unwrap();
	let mut other = session();
	other.subject = "other".into();
	assert!(state.conversation("draft", &other, 2).is_err());
	assert!(
		state
			.append(
				"draft",
				&session(),
				1,
				ChatMessage {
					role: ChatRole::User,
					body: "stale".into()
				},
				2
			)
			.is_err()
	);
	assert_eq!(
		state
			.append(
				"draft",
				&session(),
				0,
				ChatMessage {
					role: ChatRole::Assistant,
					body: "Which columns?".into()
				},
				2
			)
			.unwrap()
			.version,
		1
	);
}

fn draft() -> Conversation {
	Conversation {
		id: "draft".into(),
		subject: "user".into(),
		integration: "app".into(),
		kind: FeedbackKind::FeatureRequest,
		context: None,
		messages: vec![ChatMessage {
			role: ChatRole::User,
			body: "Export CSV".into(),
		}],
		version: 0,
		item: None,
		expires_at: 100,
	}
}

#[test]
fn registrations_are_bounded_and_salts_survive_new_integrations_and_rotation() {
	let mut state = EmbeddedState::default();
	let mut invalid = integration();
	invalid.name.clear();
	assert!(state.register(invalid, "salt").is_err());
	state.register(integration(), "salt").unwrap();
	assert_eq!(
		state.register(integration(), "other"),
		Err(EmbeddedError::Conflict)
	);
	let mut other = integration();
	other.id = "mobile".into();

	state.register(other, "different-salt").unwrap();
	assert_eq!(state.identity_salt, "salt");
	assert_eq!(
		state.rotate("missing", "digest", false),
		Err(EmbeddedError::NotFound)
	);
	state.integrations[0].generation = u32::MAX;
	assert_eq!(
		state.rotate("app", "digest", false),
		Err(EmbeddedError::Conflict)
	);
	state.integrations[0].generation = 0;
	state.rotate("app", "digest", true).unwrap();
	assert_eq!(state.integration("app"), Err(EmbeddedError::Unauthorized));
}

#[test]
fn tokens_cannot_exceed_their_lifetime_surface_or_capacity() {
	let mut state = EmbeddedState::default();
	state.register(integration(), "salt").unwrap();
	let mut invalid = session();
	invalid.expires_at = 601;
	assert!(state.issue_session(invalid, 0).is_err());
	state.integrations[0].surfaces = vec![ClientSurface::Mobile];
	assert!(state.issue_session(session(), 0).is_err());
	state.integrations[0].surfaces.push(ClientSurface::Web);

	state.sessions = vec![session(); 4096];
	assert_eq!(
		state.issue_session(session(), 0),
		Err(EmbeddedError::RateLimited)
	);
	state
		.issue_session(
			FeedbackSession {
				expires_at: 601,
				..session()
			},
			600,
		)
		.unwrap();
	assert_eq!(state.sessions.len(), 1);
}

#[test]
fn drafts_expire_and_reject_oversized_or_already_submitted_messages() {
	let mut state = EmbeddedState::default();
	let mut invalid = draft();
	invalid.messages.clear();
	assert!(state.create_conversation(invalid, 1).is_err());
	state.create_conversation(draft(), 1).unwrap();
	assert_eq!(
		state.create_conversation(draft(), 1),
		Err(EmbeddedError::RateLimited)
	);
	assert_eq!(
		state.conversation("draft", &session(), 100),
		Err(EmbeddedError::NotFound)
	);
	let message = ChatMessage {
		role: ChatRole::User,
		body: "x".repeat(3999),
	};

	assert!(state.append("draft", &session(), 0, message, 2).is_err());
	state.conversations[0].item = Some("fb-1".into());
	assert_eq!(
		state.append(
			"draft",
			&session(),
			0,
			ChatMessage {
				role: ChatRole::User,
				body: "more".into()
			},
			2
		),
		Err(EmbeddedError::Conflict)
	);
	assert!(validate_message("  ").is_err());
	assert!(validate_message(&"x".repeat(4001)).is_err());
	state
		.create_conversation(
			Conversation {
				id: "fresh".into(),
				expires_at: 200,
				..draft()
			},
			100,
		)
		.unwrap();
	assert_eq!(state.conversations.len(), 1);
}

#[test]
fn quotas_and_idempotency_history_remain_bounded() {
	let mut state = EmbeddedState::default();
	assert!(state.quota("user", 1, 0, 1).is_err());

	for index in 0..10000 {
		state.quotas.insert(index.to_string(), (100, 1));
	}

	assert_eq!(
		state.quota("new", 1, 60, 1),
		Err(EmbeddedError::RateLimited)
	);
	assert!(state.quota("1", 2, 60, 1).is_ok());
	assert!(state.submitted("u", "", "payload", 1).is_err());
	let record = SubmissionKey {
		subject: "u".into(),
		key: "key".into(),
		fingerprint: "payload".into(),
		item: "fb-1".into(),
		expires_at: 100,
	};
	state.submission_keys = vec![record.clone(); 10000];
	assert_eq!(
		state.remember_submission(record.clone(), 1),
		Err(EmbeddedError::RateLimited)
	);
	state
		.remember_submission(
			SubmissionKey {
				expires_at: 200,
				..record
			},
			100,
		)
		.unwrap();
	assert_eq!(state.submission_keys.len(), 1);
}

#[test]
fn worker_pairing_is_expiring_single_use_scoped_and_revocable() {
	let mut state = EmbeddedState::default();
	state.pairings.push(WorkerPairing {
		code_hash: "code".into(),
		name: "Local agent".into(),
		repositories: vec!["acme/web".into()],
		expires_at: 100,
	});
	let worker = Worker {
		id: "worker".into(),
		name: "Local agent".into(),
		credential_hash: "digest".into(),
		repositories: vec!["acme/web".into()],
		revoked: false,
		last_seen: 1,
	};
	assert_eq!(
		state.pair("code", worker.clone(), 100),
		Err(EmbeddedError::Unauthorized)
	);
	let mut invalid = worker.clone();
	invalid.repositories = vec!["acme/private".into()];
	assert_eq!(state.pair("code", invalid, 1), Err(EmbeddedError::Conflict));
	state.pair("code", worker.clone(), 1).unwrap();

	assert_eq!(state.worker("digest").unwrap(), &worker);
	assert_eq!(
		state.pair("code", worker, 2),
		Err(EmbeddedError::Unauthorized)
	);
	state.workers[0].revoked = true;
	assert_eq!(state.worker("digest"), Err(EmbeddedError::Unauthorized));
}

#[test]
fn source_context_never_accepts_url_queries_or_unbounded_metadata() {
	let mut context = PageContext {
		route: "/invoices".into(),
		element: None,
		app_version: Some("1.0".into()),
		locale: Some("en".into()),
	};
	assert!(validate_context(Some(&context)).is_ok());
	context.route = "/invoices?token=private".into();
	assert!(validate_context(Some(&context)).is_err());
	context.route = "/invoices".into();
	context.locale = Some("x".repeat(101));
	assert!(validate_context(Some(&context)).is_err());
}

#[test]
fn empty_draft_identifiers_and_oversized_pinned_elements_are_invalid() {
	let mut state = EmbeddedState::default();
	let mut invalid = draft();
	invalid.id.clear();
	assert!(state.create_conversation(invalid, 1).is_err());
	let mut context = PageContext {
		route: "Invoices".into(),
		app_version: None,
		locale: None,
		element: Some(crate::PinnedElement {
			selector: "export".into(),
			label: Some("Export invoices".into()),
		}),
	};
	assert!(validate_context(Some(&context)).is_ok());
	context.element.as_mut().unwrap().label = Some("x".repeat(201));
	assert!(validate_context(Some(&context)).is_err());

	context.element.as_mut().unwrap().selector = "x".repeat(201);
	assert!(validate_context(Some(&context)).is_err());
}
