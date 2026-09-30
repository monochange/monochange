//! Rule evaluation primitives for the Acme platform.
//!
//! The engine is intentionally tiny: callers register rules and ask which of
//! them match a query. Storage and transport live in the `@acme/sdk` package.

/// A single rule that matches a string key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
	/// Key the rule matches.
	pub key: String,
}

impl Rule {
	/// Build a rule for `key`.
	pub fn new(key: impl Into<String>) -> Self {
		Self { key: key.into() }
	}
}

/// Return every rule whose key contains `query`, ignoring case.
pub fn matching_rules<'a>(rules: &'a [Rule], query: &str) -> Vec<&'a Rule> {
	let needle = query.to_lowercase();
	rules
		.iter()
		.filter(|rule| rule.key.to_lowercase().contains(&needle))
		.collect()
}

/// Count the rules in a rule set.
pub fn rule_count(rules: &[Rule]) -> usize {
	rules.len()
}
