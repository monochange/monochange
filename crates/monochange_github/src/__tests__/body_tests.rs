use super::*;

#[test]
fn boundaries_ignore_code_fences_and_nested_headings() {
	let body = "intro\n## Features\n### One\n```md\n### fake\n~~~\n```\n  ## nested\n### Two\n~~~~\n## fake\n~~~\n~~~~\n- entry\n";
	let prefixes = entry_boundaries(body)
		.into_iter()
		.map(|end| body.get(..end).unwrap_or_default())
		.collect::<Vec<_>>();
	insta::assert_snapshot!(prefixes.join("\n--- boundary ---\n"));
	assert_eq!(entry_boundaries(""), [] as [usize; 0]);
}
