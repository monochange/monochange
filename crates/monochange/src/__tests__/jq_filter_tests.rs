#![allow(clippy::disallowed_methods)]
use super::*;

#[test]
fn jq_filter_handles_not_equal_and_string_escapes() {
	let output = r#"{"items":[{"id":"main","name":"a\"b","tags":["x"],"candy":true},{"id":"draft","name":"plain","tags":["y"],"candy":true}]}"#;

	assert_eq!(
		apply_jq_filter(output, r#".items[] | select(.id != "draft") | .id"#).unwrap(),
		"main"
	);
	assert_eq!(
		apply_jq_filter(output, r#".items[] | select("a\"b" == .name) | .id"#).unwrap(),
		"main"
	);
	assert_eq!(
		apply_jq_filter(
			output,
			r#".items[] | select(.tags[0] == "x" and .candy == true and .name == "a\"b") | .id"#
		)
		.unwrap(),
		"main"
	);
}
