// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::releases::load_releases;

#[tokio::test]
async fn generated_history_renders_links_and_safe_formatted_notes() {
	let directory = tempfile::tempdir().unwrap();
	let fixture = std::fs::read_to_string(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../../../fixtures/tests/website-releases/0.2.0.json"
	))
	.unwrap();
	std::fs::write(
		directory.path().join("0.2.0.json"),
		fixture.replace("Read website updates", "<script>unsafe summary</script>"),
	)
	.unwrap();
	let releases = load_releases(directory.path()).await.unwrap();
	let html = Owner::new().with(|| release_history(Ok(releases)).to_html());
	assert!(html.contains("v0.2.0"));
	assert!(html.contains("href=\"/releases/0.2.0.json\""));
	assert!(html.contains("&lt;script&gt;unsafe summary&lt;/script&gt;"));
	assert!(html.contains("<strong>your website</strong>"));
	assert!(!html.contains("<script"));
	assert!(!html.contains("javascript:"));
	assert!(!html.contains("onerror"));
}

#[test]
fn empty_history_and_unavailable_history_have_distinct_visible_states() {
	let empty = Owner::new().with(|| release_history(Ok(Vec::new())).to_html());
	assert!(empty.contains("Our first versioned release is on its way."));
	assert!(!empty.contains("role=\"alert\""));
	let unavailable = Owner::new().with(|| {
		release_history(Err(server_fn::ServerFnError::new(
			"Release notes are unavailable right now.",
		)))
		.to_html()
	});
	assert!(unavailable.contains("role=\"alert\""));
	assert!(unavailable.contains("Release notes are unavailable right now."));
	assert!(!unavailable.contains("Our first versioned release"));
}
