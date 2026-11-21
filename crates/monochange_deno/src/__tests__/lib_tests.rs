use std::path::Path;

#[rstest::rstest]
#[case("{\"version\":\"1.2.3\"} /* unfinished")]
#[case("{\"value\":1/* gap */2}")]
#[case("{\"value\":true/* gap */false}")]
#[case("{\"value\":/1}")]
fn rejects_malformed_jsonc_comments_and_separated_tokens(#[case] contents: &str) {
	assert!(super::parse_manifest_contents(contents).is_err());
}

#[test]
fn validates_versioned_jsonc_with_comments_and_trailing_commas() {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/deno/jsonc-init/workspace/deno.jsonc");

	super::validate_versioned_file(&path, "deno.jsonc", None)
		.unwrap_or_else(|error| panic!("validate versioned JSONC: {error}"));
}
