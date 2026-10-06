// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use super::*;

fn fixture() -> String {
	std::fs::read_to_string(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../../../fixtures/tests/website-releases/0.2.0.json"
	))
	.unwrap()
}

#[tokio::test]
async fn history_sorts_semver_and_sanitizes_markdown() {
	let directory = tempfile::tempdir().unwrap();
	for version in ["0.2.0", "0.10.0"] {
		std::fs::write(
			directory.path().join(format!("{version}.json")),
			fixture().replace("0.2.0", version),
		)
		.unwrap();
	}
	std::fs::write(directory.path().join("README.md"), "not release data").unwrap();
	let releases = load_releases(directory.path()).await.unwrap();
	assert_eq!(
		releases
			.iter()
			.map(|release| release.version.as_str())
			.collect::<Vec<_>>(),
		["0.10.0", "0.2.0"]
	);
	let entry = &releases[0].sections[0].entries[0];
	assert_eq!(entry.summary, "Read website updates");
	assert!(entry.details_html.contains("<strong>your website</strong>"));
	assert!(
		entry
			.details_html
			.contains("https://monochange.dev/changelog")
	);
	assert!(!entry.details_html.contains("<script"));
	assert!(!entry.details_html.contains("javascript:"));
	assert!(!entry.details_html.contains("onerror"));
}

#[tokio::test]
async fn first_release_has_an_empty_history() {
	let directory = tempfile::tempdir().unwrap();
	assert!(load_releases(directory.path()).await.unwrap().is_empty());
	assert!(
		load_releases(&directory.path().join("not-created-yet"))
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn corrupt_or_misrouted_notes_fail_instead_of_disappearing() {
	let directory = tempfile::tempdir().unwrap();
	let path = directory.path().join("0.2.0.json");
	for content in [
		"invalid JSON".to_owned(),
		fixture().replace("website", "default"),
		fixture().replace("\"title\": \"0.2.0\"", "\"title\": \"0.3.0\""),
	] {
		std::fs::write(&path, content).unwrap();
		assert!(load_releases(directory.path()).await.is_err());
	}
	std::fs::remove_file(&path).unwrap();
	std::fs::write(
		directory.path().join("0.2.0-alpha.1.json"),
		fixture().replace("0.2.0", "0.2.0-alpha.1"),
	)
	.unwrap();
	assert!(load_releases(directory.path()).await.is_err());
}

#[tokio::test]
async fn invalid_filename_or_unreadable_destination_fails() {
	let directory = tempfile::tempdir().unwrap();
	let path = directory.path().join("invalid.json");
	std::fs::write(&path, fixture()).unwrap();
	assert!(load_releases(directory.path()).await.is_err());
	assert!(load_releases(&path).await.is_err());
}

#[test]
fn summary_stays_text_and_absent_details_are_empty() {
	let notes = serde_json::from_str(
		&fixture().replace("\"Read website updates\"", "\"<script>alert(1)</script>\""),
	)
	.unwrap();
	let release = present_release(notes);
	assert_eq!(
		release.sections[0].entries[0].summary,
		"<script>alert(1)</script>"
	);
	assert!(release.sections[0].entries[1].details_html.is_empty());
}
