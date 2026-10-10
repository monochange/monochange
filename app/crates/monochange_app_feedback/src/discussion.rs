//! Who acts on a feedback item, and the conversation between submitters,
//! the triage agent, and maintainers.

use serde::Deserialize;
use serde::Serialize;

use crate::submission::Attachment;

/// Everyone who can act on an item. Authority rules in the pipeline match on
/// this type, so it is the single place where "who may do what" is decided.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum Actor {
	System,
	Ai,
	User(String),
	Maintainer(String),
}

impl Actor {
	pub fn is_maintainer(&self) -> bool {
		matches!(self, Actor::Maintainer(_))
	}

	/// System and AI actors run automation; they may triage and record
	/// artifacts but never decide.
	pub fn is_automation(&self) -> bool {
		matches!(self, Actor::System | Actor::Ai)
	}
}

/// One entry in an item's discussion thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscussionMessage {
	pub author: Actor,
	pub body: String,
	pub attachments: Vec<Attachment>,
	/// Set when screening flagged the message. Held messages stay visible to
	/// maintainers but are never shown to other users or fed to triage.
	pub held: bool,
}

/// Messages triage may read: everything that screening did not hold.
pub fn visible_messages(
	discussion: &[DiscussionMessage],
) -> impl Iterator<Item = &DiscussionMessage> {
	discussion.iter().filter(|message| !message.held)
}

#[cfg(test)]
#[path = "__tests__/discussion_tests.rs"]
mod tests;
