//! Cheap duplicate detection, so the widget can say "someone already asked
//! for this — vote instead" before a near-identical request is filed.
//!
//! Word-set overlap is crude next to embeddings, but it is deterministic,
//! explainable, and good enough to catch the common case of several people
//! describing the same thing in similar words. An AI engine can replace it
//! behind the same function signature.

use std::collections::BTreeSet;

/// Words too common to signal what a request is about.
const STOPWORDS: [&str; 40] = [
	"the", "and", "for", "that", "this", "with", "you", "are", "was", "but", "not", "have", "has",
	"can", "could", "would", "should", "when", "what", "there", "from", "they", "them", "then",
	"than", "into", "your", "our", "its", "it's", "please", "add", "want", "like", "just", "also",
	"some", "able", "way", "app",
];

/// The normalized content words of `text`.
pub fn keywords(text: &str) -> BTreeSet<String> {
	text.split(|character: char| !character.is_alphanumeric() && character != '\'')
		.map(str::to_lowercase)
		.filter(|word| word.chars().count() >= 3 && !STOPWORDS.contains(&word.as_str()))
		.map(|word| word.trim_end_matches('s').to_owned())
		.filter(|word| word.chars().count() >= 3)
		.collect()
}

/// Jaccard similarity of two keyword sets as a whole percentage.
pub fn similarity_percent(left: &BTreeSet<String>, right: &BTreeSet<String>) -> u8 {
	let union = left.union(right).count();
	if union == 0 {
		return 0;
	}
	let shared = left.intersection(right).count();
	u8::try_from(shared * 100 / union).unwrap_or(100)
}

#[cfg(test)]
#[path = "__tests__/similarity_tests.rs"]
mod tests;
