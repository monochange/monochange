//! Client operations derive project and actor from verified session credentials.

use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use monochange_app_feedback::Actor;
use monochange_app_feedback::FeedbackKind;
use monochange_app_feedback::FeedbackSubmission;
use monochange_app_feedback::PageContext;
use monochange_app_feedback::RegisteredApp;
use monochange_app_feedback::SubmitterIdentity;
use monochange_app_feedback::embedded::ChatMessage;
use monochange_app_feedback::embedded::ChatRole;
use monochange_app_feedback::embedded::ClientSurface;
use monochange_app_feedback::embedded::Conversation;
use monochange_app_feedback::embedded::EmbeddedError;
use monochange_app_feedback::embedded::FeedbackSession;
use monochange_app_feedback::embedded::SourceContext;
use monochange_app_feedback::embedded::SubmissionKey;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use super::ApiFailure;
use super::ApiResult;
use super::Parameters;
use super::assistant;
use super::auth;
use super::credential;
use super::fingerprint;
use super::now;
use crate::AppState;
use crate::feedback;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionInput {
	user_id: String,
	surface: ClientSurface,
}

pub(super) async fn session(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<SessionInput>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;

	if input.user_id.trim().is_empty()
		|| input.user_id.len() > 200
		|| headers.contains_key("origin")
		|| headers.contains_key("sec-fetch-site")
	{
		return Err(ApiFailure::invalid(
			"sessions must be issued by the host backend for an authenticated user",
		));
	}
	let scope = auth::scope(&state, path.project).await?;
	let digest = auth::bearer(&headers, "mcfi", path.project)?;
	let token = credential("mcfs", path.project);
	let time = now();
	let expires = time + 600;
	feedback::update(&state, &scope, |service| {
		let app = service
			.embedded()
			.integrations
			.iter()
			.find(|app| !app.revoked && app.credential_hash == digest)
			.ok_or(EmbeddedError::Unauthorized)?
			.clone();
		let subject = fingerprint(
			&json!([
				service.embedded().identity_salt,
				app.namespace,
				input.user_id
			])
			.to_string(),
		);
		service
			.embedded_mut()
			.quota(&format!("sessions:{}", app.id), 1000, 3600, time)?;
		service
			.embedded_mut()
			.quota(&format!("sessions:{subject}"), 30, 3600, time)?;
		service.embedded_mut().issue_session(
			FeedbackSession {
				token_hash: fingerprint(&token),
				integration: app.id,
				subject,
				surface: input.surface,
				generation: app.generation,
				expires_at: expires,
			},
			time,
		)?;
		Ok(())
	})
	.await?;
	Ok(Json(json!({"token":token,"expires_at":expires})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SuggestInput {
	text: String,
}

pub(super) async fn suggestions(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<SuggestInput>, JsonRejection>,
) -> ApiResult<assistant::AssistantReply> {
	let input = super::json(input)?;
	monochange_app_feedback::embedded::validate_message(&input.text)?;
	let scope = auth::scope(&state, path.project).await?;
	let session = auth::mutate(&state, &scope, &headers, "assistant", 60, |_, session| {
		Ok(session.clone())
	})
	.await?;
	let loaded = feedback::load(&state, &scope).await?;
	Ok(Json(
		assistant::respond(
			&state,
			&loaded.service,
			&session.subject,
			&[ChatMessage {
				role: ChatRole::User,
				body: input.text,
			}],
		)
		.await,
	))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftInput {
	kind: FeedbackKind,
	message: String,
	context: Option<PageContext>,
}

pub(super) async fn create_conversation(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<DraftInput>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;
	let scope = auth::scope(&state, path.project).await?;
	let id = uuid::Uuid::new_v4().to_string();
	let draft = auth::mutate(
		&state,
		&scope,
		&headers,
		"assistant",
		60,
		|service, session| {
			let draft = Conversation {
				id: id.clone(),
				subject: session.subject.clone(),
				integration: session.integration.clone(),
				kind: input.kind,
				context: input.context.clone(),
				messages: vec![ChatMessage {
					role: ChatRole::User,
					body: input.message.clone(),
				}],
				version: 0,
				item: None,
				expires_at: now() + 30 * 86400,
			};
			service
				.embedded_mut()
				.create_conversation(draft.clone(), now())?;
			Ok(draft)
		},
	)
	.await?;
	respond_to_draft(&state, &scope, &headers, draft).await
}

async fn respond_to_draft(
	state: &AppState,
	scope: &feedback::ProjectScope,
	headers: &HeaderMap,
	draft: Conversation,
) -> ApiResult<Value> {
	let loaded = feedback::load(state, scope).await?;
	let reply = assistant::respond(state, &loaded.service, &draft.subject, &draft.messages).await;
	let conversation = auth::mutate(
		state,
		scope,
		headers,
		"draft_write",
		240,
		|service, session| {
			service
				.embedded_mut()
				.append(
					&draft.id,
					session,
					draft.version,
					ChatMessage {
						role: ChatRole::Assistant,
						body: reply.reply.clone(),
					},
					now(),
				)
				.map_err(Into::into)
		},
	)
	.await?;
	Ok(Json(
		json!({"conversation":conversation,"similar":reply.similar,"mode":reply.mode}),
	))
}

pub(super) async fn show_conversation(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
) -> ApiResult<Value> {
	let scope = auth::scope(&state, path.project).await?;
	let id = path.id()?;
	let conversation = auth::mutate(&state, &scope, &headers, "read", 240, |service, session| {
		Ok(service.embedded().conversation(id, session, now())?.clone())
	})
	.await?;
	Ok(Json(json!({"conversation":conversation})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessageInput {
	body: String,
	version: u32,
}

pub(super) async fn conversation_message(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<MessageInput>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;
	let scope = auth::scope(&state, path.project).await?;
	let id = path.id()?;
	let draft = auth::mutate(
		&state,
		&scope,
		&headers,
		"assistant",
		60,
		|service, session| {
			service
				.embedded_mut()
				.append(
					id,
					session,
					input.version,
					ChatMessage {
						role: ChatRole::User,
						body: input.body.clone(),
					},
					now(),
				)
				.map_err(Into::into)
		},
	)
	.await?;
	respond_to_draft(&state, &scope, &headers, draft).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubmitInput {
	conversation: String,
	version: u32,
}

pub(super) async fn submit(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<SubmitInput>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;
	let key = headers
		.get("idempotency-key")
		.and_then(|value| value.to_str().ok())
		.ok_or_else(|| ApiFailure::invalid("Idempotency-Key is required"))?;
	let scope = auth::scope(&state, path.project).await?;
	let fingerprint = fingerprint(&json!([input.conversation, input.version]).to_string());
	let result = auth::mutate(&state, &scope, &headers, "submit", 20, |service, session| {
		if let Some(id) = service.embedded().submitted(&session.subject, key, &fingerprint, now())? {
			let item = service.item(&id).ok_or(EmbeddedError::NotFound)?;
			return Ok(json!({"id":id,"stage":item.stage}));
		}

		let draft = service.embedded().conversation(&input.conversation, session, now())?.clone();
		if draft.version != input.version { return Err(EmbeddedError::Conflict.into()); }
		if let Some(id) = draft.item { return Ok(json!({"id":id,"stage":service.item(&id).ok_or(EmbeddedError::NotFound)?.stage})); }
		let app = service.embedded().integration(&session.integration)?.clone();
		service.register_app(RegisteredApp { slug: app.id.clone(), display_name: app.name, allowed_origins: app.origins });
		let user_messages: Vec<&str> = draft.messages.iter().filter(|message| message.role == ChatRole::User).map(|message| message.body.as_str()).collect();
		let description = user_messages.join("\n\n");
		let receipt = service.receive(FeedbackSubmission { kind: draft.kind, description, page: draft.context, attachments: Vec::new(), submitter: SubmitterIdentity { anonymous_id: session.subject.clone(), email: None }, app_slug: session.integration.clone() })?;
		// Answers gathered privately are replayed through ordinary triage, never accepted automatically.
		if user_messages.len() > 1 && receipt.stage == monochange_app_feedback::Stage::Discussing {
			service.reply(&receipt.id, &Actor::User(session.subject.clone()), user_messages.last().copied().unwrap_or_default(), Vec::new())?;
		}

		service.embedded_mut().sources.insert(receipt.id.clone(), SourceContext { integration: session.integration.clone(), surface: session.surface });
		service.embedded_mut().remember_submission(SubmissionKey { subject: session.subject.clone(), key: key.into(), fingerprint: fingerprint.clone(), item: receipt.id.clone(), expires_at: now() + 86400 }, now())?;
		let conversation = service.embedded_mut().conversations.iter_mut().find(|draft| draft.id == input.conversation).ok_or(EmbeddedError::NotFound)?;
		conversation.item = Some(receipt.id.clone());
		Ok(json!({"id":receipt.id,"stage":service.item(&receipt.id).ok_or(EmbeddedError::NotFound)?.stage}))
	}).await?;
	super::admin::schedule_after_write(&state, &scope);
	Ok(Json(result))
}

fn visible(
	service: &monochange_app_feedback::FeedbackService<monochange_app_feedback::RuleBasedTriage>,
	item: &str,
	subject: &str,
) -> Result<(), EmbeddedError> {
	let own = service
		.item(item)
		.is_some_and(|item| item.submission.submitter.anonymous_id == subject);

	if !own
		&& !service
			.status_feed()
			.feed
			.roadmap
			.iter()
			.any(|entry| entry.id == item)
	{
		return Err(EmbeddedError::NotFound);
	}
	Ok(())
}

pub(super) async fn item(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
) -> ApiResult<Value> {
	let scope = auth::scope(&state, path.project).await?;
	let id = path.id()?;
	let result = auth::mutate(&state, &scope, &headers, "read", 240, |service, session| {
		visible(service, id, &session.subject)?;
		let item = service.item(id).ok_or(EmbeddedError::NotFound)?;
		let entry = service.status_feed().feed.roadmap.into_iter().find(|entry| entry.id == id);
		Ok(json!({"id":id,"stage":item.stage,"title":entry.as_ref().map_or("Request awaiting review", |entry| entry.title.as_str()),"status":monochange_app_feedback::roadmap::public_status_line(item.stage),"votes":item.votes.total(),"voted":item.votes.has_voted(&session.subject),"messages":service.public_thread(id)?,"update":service.public_item_update(id).ok()}))
	}).await?;
	Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReplyInput {
	body: String,
}

pub(super) async fn reply(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
	input: Result<Json<ReplyInput>, JsonRejection>,
) -> ApiResult<Value> {
	let input = super::json(input)?;
	monochange_app_feedback::embedded::validate_message(&input.body)?;
	let scope = auth::scope(&state, path.project).await?;
	let id = path.id()?;
	let next_stage = auth::mutate(&state, &scope, &headers, "reply", 60, |service, session| {
		visible(service, id, &session.subject)?;
		service.reply(
			id,
			&Actor::User(session.subject.clone()),
			&input.body,
			Vec::new(),
		)
	})
	.await?;
	super::admin::schedule_after_write(&state, &scope);
	Ok(Json(json!({"id":id,"stage":next_stage})))
}

pub(super) async fn vote(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
) -> ApiResult<Value> {
	change_vote(&state, path, &headers, true).await
}
pub(super) async fn unvote(
	State(state): State<AppState>,
	Path(path): Path<Parameters>,
	headers: HeaderMap,
) -> ApiResult<Value> {
	change_vote(&state, path, &headers, false).await
}

async fn change_vote(
	state: &AppState,
	path: Parameters,
	headers: &HeaderMap,
	on: bool,
) -> ApiResult<Value> {
	let scope = auth::scope(state, path.project).await?;
	let id = path.id()?;
	let votes = auth::mutate(state, &scope, headers, "vote", 120, |service, session| {
		if !service
			.status_feed()
			.feed
			.roadmap
			.iter()
			.any(|entry| entry.id == id)
		{
			return Err(EmbeddedError::NotFound.into());
		}
		if let Some(item) = service.item(id)
			&& item.votes.has_voted(&session.subject) == on
		{
			Ok(item.votes.total())
		} else if on {
			service.vote(id, &session.subject)
		} else {
			service.retract_vote(id, &session.subject)
		}
	})
	.await?;
	Ok(Json(json!({"id":id,"votes":votes,"voted":on})))
}
