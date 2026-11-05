//! Host-attested identities, bounded private drafts, credentials, and durable quotas.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::FeedbackKind;
use crate::PageContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSurface {
	Web,
	Mobile,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EmbeddedError {
	#[error("invalid feedback input: {0}")]
	Invalid(&'static str),
	#[error("feedback credentials are invalid, expired, or revoked")]
	Unauthorized,
	#[error("no such feedback resource")]
	NotFound,
	#[error("the feedback request changed; reload and try again")]
	Conflict,
	#[error("feedback quota exceeded; try again later")]
	RateLimited,
}

/// Backend credential digests never leave maintainer-facing storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integration {
	pub id: String,
	pub name: String,
	pub namespace: String,
	pub origins: Vec<String>,
	pub surfaces: Vec<ClientSurface>,
	pub credential_hash: String,
	pub generation: u32,
	pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackSession {
	pub token_hash: String,
	pub integration: String,
	pub subject: String,
	pub surface: ClientSurface,
	pub generation: u32,
	pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
	User,
	Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
	pub role: ChatRole,
	pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
	pub id: String,
	pub subject: String,
	pub integration: String,
	pub kind: FeedbackKind,
	pub context: Option<PageContext>,
	pub messages: Vec<ChatMessage>,
	pub version: u32,
	pub item: Option<String>,
	pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceContext {
	pub integration: String,
	pub surface: ClientSurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionKey {
	pub subject: String,
	pub key: String,
	pub fingerprint: String,
	pub item: String,
	pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worker {
	pub id: String,
	pub name: String,
	pub credential_hash: String,
	pub repositories: Vec<String>,
	pub revoked: bool,
	pub last_seen: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerPairing {
	pub code_hash: String,
	pub name: String,
	pub repositories: Vec<String>,
	pub expires_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedState {
	/// Persistent project salt keeps pseudonyms stable across credential rotation.
	pub identity_salt: String,
	pub integrations: Vec<Integration>,
	pub sessions: Vec<FeedbackSession>,
	pub conversations: Vec<Conversation>,
	pub submission_keys: Vec<SubmissionKey>,
	pub sources: BTreeMap<String, SourceContext>,
	pub workers: Vec<Worker>,
	pub pairings: Vec<WorkerPairing>,
	pub quotas: BTreeMap<String, (i64, u32)>,
}

impl EmbeddedState {
	pub fn integration(&self, id: &str) -> Result<&Integration, EmbeddedError> {
		self.integrations
			.iter()
			.find(|app| app.id == id && !app.revoked)
			.ok_or(EmbeddedError::Unauthorized)
	}

	pub fn register(&mut self, integration: Integration, salt: &str) -> Result<(), EmbeddedError> {
		if integration.id.is_empty()
			|| integration.name.trim().is_empty()
			|| integration.name.len() > 120
			|| integration.namespace.is_empty()
			|| integration.namespace.len() > 120
			|| integration.surfaces.is_empty()
			|| integration.surfaces.len() > 2
			|| integration.origins.len() > 20
			|| integration.credential_hash.is_empty()
			|| salt.is_empty()
		{
			return Err(EmbeddedError::Invalid("integration"));
		}

		if self.integrations.len() >= 32
			|| self
				.integrations
				.iter()
				.any(|existing| existing.id == integration.id)
		{
			return Err(EmbeddedError::Conflict);
		}

		if self.identity_salt.is_empty() {
			salt.clone_into(&mut self.identity_salt);
		}

		self.integrations.push(integration);
		Ok(())
	}

	/// Rotating or revoking an integration also invalidates all its sessions.
	pub fn rotate(
		&mut self,
		id: &str,
		credential_hash: &str,
		revoke: bool,
	) -> Result<(), EmbeddedError> {
		let app = self
			.integrations
			.iter_mut()
			.find(|app| app.id == id)
			.ok_or(EmbeddedError::NotFound)?;
		app.generation = app
			.generation
			.checked_add(1)
			.ok_or(EmbeddedError::Conflict)?;
		app.revoked = revoke;
		credential_hash.clone_into(&mut app.credential_hash);
		Ok(())
	}

	pub fn session(&self, digest: &str, now: i64) -> Result<FeedbackSession, EmbeddedError> {
		let session = self
			.sessions
			.iter()
			.find(|session| session.token_hash == digest && session.expires_at > now)
			.ok_or(EmbeddedError::Unauthorized)?;
		let app = self.integration(&session.integration)?;

		if app.generation != session.generation {
			return Err(EmbeddedError::Unauthorized);
		}

		Ok(session.clone())
	}

	pub fn issue_session(
		&mut self,
		session: FeedbackSession,
		now: i64,
	) -> Result<(), EmbeddedError> {
		let app = self.integration(&session.integration)?;

		if !app.surfaces.contains(&session.surface)
			|| app.generation != session.generation
			|| session.subject.is_empty()
			|| session.token_hash.is_empty()
			|| session.expires_at <= now
			|| session.expires_at > now + 600
		{
			return Err(EmbeddedError::Invalid("feedback session"));
		}
		self.sessions.retain(|session| session.expires_at > now);

		if self.sessions.len() >= 4096 {
			return Err(EmbeddedError::RateLimited);
		}

		self.sessions.push(session);
		Ok(())
	}

	/// Fixed windows are persisted, so refreshing tokens or restarting cannot reset them.
	pub fn quota(
		&mut self,
		key: &str,
		maximum: u32,
		window: i64,
		now: i64,
	) -> Result<(), EmbeddedError> {
		if window <= 0 {
			return Err(EmbeddedError::Invalid("quota window"));
		}

		let period = now.div_euclid(window);
		self.quotas.retain(|_, (expires, _)| *expires > now);

		if self.quotas.len() >= 10_000 && !self.quotas.contains_key(key) {
			return Err(EmbeddedError::RateLimited);
		}

		let count = self
			.quotas
			.entry(key.into())
			.or_insert(((period + 1) * window, 0));

		if count.1 >= maximum {
			return Err(EmbeddedError::RateLimited);
		}

		count.1 += 1;
		Ok(())
	}

	pub fn conversation(
		&self,
		id: &str,
		session: &FeedbackSession,
		now: i64,
	) -> Result<&Conversation, EmbeddedError> {
		self.conversations
			.iter()
			.find(|draft| {
				draft.id == id
					&& draft.subject == session.subject
					&& draft.integration == session.integration
					&& draft.expires_at > now
			})
			.ok_or(EmbeddedError::NotFound)
	}

	pub fn create_conversation(
		&mut self,
		draft: Conversation,
		now: i64,
	) -> Result<(), EmbeddedError> {
		let [message] = draft.messages.as_slice() else {
			return Err(EmbeddedError::Invalid("conversation"));
		};

		if draft.id.is_empty() || draft.subject.is_empty() || message.role != ChatRole::User {
			return Err(EmbeddedError::Invalid("conversation"));
		}

		validate_message(&message.body)?;
		validate_context(draft.context.as_ref())?;
		self.conversations.retain(|draft| draft.expires_at > now);

		if self.conversations.len() >= 2048
			|| self
				.conversations
				.iter()
				.any(|existing| existing.id == draft.id)
		{
			return Err(EmbeddedError::RateLimited);
		}
		self.conversations.push(draft);
		Ok(())
	}

	pub fn append(
		&mut self,
		id: &str,
		session: &FeedbackSession,
		expected: u32,
		message: ChatMessage,
		now: i64,
	) -> Result<Conversation, EmbeddedError> {
		validate_message(&message.body)?;
		self.conversation(id, session, now)?;
		let draft = self
			.conversations
			.iter_mut()
			.find(|draft| draft.id == id)
			.ok_or(EmbeddedError::NotFound)?;

		if message.role == ChatRole::User
			&& draft
				.messages
				.iter()
				.filter(|message| message.role == ChatRole::User)
				.map(|message| message.body.chars().count() + 2)
				.sum::<usize>()
				+ message.body.chars().count()
				> 4000
		{
			return Err(EmbeddedError::Invalid(
				"keep the complete report under 4000 characters",
			));
		}

		if draft.version != expected || draft.item.is_some() || draft.messages.len() >= 40 {
			return Err(EmbeddedError::Conflict);
		}

		draft.messages.push(message);
		draft.version += 1;
		Ok(draft.clone())
	}

	pub fn submitted(
		&self,
		subject: &str,
		key: &str,
		fingerprint: &str,
		now: i64,
	) -> Result<Option<String>, EmbeddedError> {
		if key.is_empty() || key.len() > 120 {
			return Err(EmbeddedError::Invalid("idempotency key"));
		}

		let Some(previous) = self.submission_keys.iter().find(|previous| {
			previous.subject == subject && previous.key == key && previous.expires_at > now
		}) else {
			return Ok(None);
		};

		if previous.fingerprint != fingerprint {
			return Err(EmbeddedError::Conflict);
		}

		Ok(Some(previous.item.clone()))
	}

	pub fn remember_submission(
		&mut self,
		record: SubmissionKey,
		now: i64,
	) -> Result<(), EmbeddedError> {
		self.submission_keys.retain(|key| key.expires_at > now);

		if self.submission_keys.len() >= 10_000 {
			return Err(EmbeddedError::RateLimited);
		}

		self.submission_keys.push(record);
		Ok(())
	}

	pub fn pair(&mut self, code_hash: &str, worker: Worker, now: i64) -> Result<(), EmbeddedError> {
		let (position, pairing) = self
			.pairings
			.iter()
			.enumerate()
			.find(|(_, pairing)| pairing.code_hash == code_hash && pairing.expires_at > now)
			.ok_or(EmbeddedError::Unauthorized)?;

		if self.workers.len() >= 32
			|| worker.repositories != pairing.repositories
			|| worker.name != pairing.name
			|| worker.credential_hash.is_empty()
			|| self.workers.iter().any(|existing| existing.id == worker.id)
		{
			return Err(EmbeddedError::Conflict);
		}
		self.pairings.remove(position);
		self.workers.push(worker);
		Ok(())
	}

	pub fn worker(&self, digest: &str) -> Result<&Worker, EmbeddedError> {
		self.workers
			.iter()
			.find(|worker| worker.credential_hash == digest && !worker.revoked)
			.ok_or(EmbeddedError::Unauthorized)
	}
}

pub fn validate_message(message: &str) -> Result<(), EmbeddedError> {
	if message.trim().is_empty() || message.chars().count() > 4000 {
		return Err(EmbeddedError::Invalid("message length"));
	}

	Ok(())
}

pub fn validate_context(context: Option<&PageContext>) -> Result<(), EmbeddedError> {
	if let Some(context) = context
		&& (context.route.is_empty()
			|| context.route.len() > 500
			|| context.route.contains(['?', '#', '\n', '\r'])
			|| [&context.app_version, &context.locale]
				.into_iter()
				.flatten()
				.any(|value| value.len() > 100)
			|| context.element.as_ref().is_some_and(|element| {
				element.selector.len() > 200
					|| element
						.label
						.as_ref()
						.is_some_and(|label| label.len() > 200)
			})) {
		return Err(EmbeddedError::Invalid("screen context"));
	}
	Ok(())
}

#[cfg(test)]
#[path = "__tests__/embedded_tests.rs"]
mod tests;
