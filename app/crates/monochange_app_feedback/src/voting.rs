//! Vote tallying. Votes signal demand ("I want this" for features, "this
//! affects me too" for bugs); they never accept work on their own —
//! acceptance stays with maintainers.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VotingRules {
	/// Votes needed before a maintainer can accept without recording an
	/// override rationale.
	pub acceptance_threshold: u32,
}

impl Default for VotingRules {
	fn default() -> Self {
		Self {
			acceptance_threshold: 3,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum VotingOutcome {
	BelowThreshold { votes: u32 },
	ThresholdReached { votes: u32 },
}

/// One vote per anonymous submitter id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteTally {
	recorded: BTreeSet<String>,
}

impl VoteTally {
	/// Records a vote, returning `false` when the submitter already voted.
	pub fn record(&mut self, submitter_id: &str) -> bool {
		self.recorded.insert(submitter_id.to_owned())
	}

	/// Removes a vote, returning `false` when the submitter had not voted.
	pub fn retract(&mut self, submitter_id: &str) -> bool {
		self.recorded.remove(submitter_id)
	}

	pub fn has_voted(&self, submitter_id: &str) -> bool {
		self.recorded.contains(submitter_id)
	}

	pub fn voters(&self) -> impl Iterator<Item = &str> {
		self.recorded.iter().map(String::as_str)
	}

	pub fn total(&self) -> u32 {
		u32::try_from(self.recorded.len()).unwrap_or(u32::MAX)
	}
}

pub fn evaluate(tally: &VoteTally, rules: &VotingRules) -> VotingOutcome {
	let votes = tally.total();
	if votes >= rules.acceptance_threshold {
		VotingOutcome::ThresholdReached { votes }
	} else {
		VotingOutcome::BelowThreshold { votes }
	}
}

#[cfg(test)]
#[path = "__tests__/voting_tests.rs"]
mod tests;
