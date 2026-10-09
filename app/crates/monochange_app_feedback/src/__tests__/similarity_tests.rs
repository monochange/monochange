use crate::similarity::keywords;
use crate::similarity::similarity_percent;

#[test]
fn keywords_drop_stopwords_short_words_and_plurals() {
	let words = keywords("Please add a dark theme for the reports, and themes!");
	assert_eq!(
		words.into_iter().collect::<Vec<_>>(),
		["dark", "report", "theme"]
	);
}

#[test]
fn plural_stripping_never_leaves_fragments() {
	assert!(keywords("ass gas").is_empty());
}

#[test]
fn similarity_is_a_jaccard_percentage() {
	let left = keywords("dark theme for reports");
	assert_eq!(similarity_percent(&left, &left), 100);
	assert_eq!(
		similarity_percent(&left, &keywords("export invoices csv")),
		0
	);
	assert_eq!(similarity_percent(&left, &keywords("dark mode")), 25);
	assert_eq!(similarity_percent(&keywords(""), &keywords("the")), 0);
}
