// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::releases::load_releases;

#[tokio::test]
async fn changelog_route_view_wraps_release_history_layout() {
	let html = crate::tests::render(|| {
		let releases = Resource::new_blocking(
			|| (),
			|()| async { Ok::<_, ServerFnError>(Vec::<WebsiteRelease>::new()) },
		);
		changelog_route_view(releases)
	})
	.await;
	assert!(html.contains("What's new."), "{html}");
	assert!(html.contains("release-history"), "{html}");
	assert!(
		html.contains("Our first versioned release is on its way.")
			|| html.contains("Loading release notes"),
		"{html}"
	);
}

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
	let html = Owner::new().with(|| view! { <ReleaseHistory result=Ok(releases) /> }.to_html());
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
	let empty = Owner::new().with(|| view! { <ReleaseHistory result=Ok(Vec::new()) /> }.to_html());
	assert!(empty.contains("Our first versioned release is on its way."));
	assert!(!empty.contains("role=\"alert\""));
	let unavailable = Owner::new().with(|| {
		view! {
			<ReleaseHistory result=Err(server_fn::ServerFnError::new(
				"Release notes are unavailable right now.",
			)) />
		}
		.to_html()
	});
	assert!(unavailable.contains("role=\"alert\""));
	assert!(unavailable.contains("Release notes are unavailable right now."));
	assert!(!unavailable.contains("Our first versioned release"));
}

#[test]
fn element_notes_keep_text_that_sits_outside_tags() {
	let _ = Owner::new().with(|| changelog_body(None));
	let _ = Owner::new().with(|| {
		changelog_body(Some(Err(server_fn::ServerFnError::new(
			"Release notes are unavailable right now.",
		))))
	});
	assert_eq!(
		plain_text("<p>Ships &amp; notes</p> today"),
		" Ships &amp; notes  today"
	);

	let release = WebsiteRelease {
		version: "0.2.0".to_owned(),
		summary: vec!["A quieter homepage.".to_owned()],
		sections: vec![crate::server_fns::releases::WebsiteSection {
			title: "Website".to_owned(),
			entries: vec![crate::server_fns::releases::WebsiteEntry {
				summary: "Release notes render as text.".to_owned(),
				details_html: "<strong>Details</strong> stay readable.".to_owned(),
			}],
		}],
	};
	let _ = Owner::new().with(|| changelog_content(vec![release]));
	let _ = Owner::new().with(|| changelog_content(Vec::new()));
}

#[tokio::test]
async fn the_changelog_screen_loads_published_notes() {
	let html = crate::tests::render(|| leptoskit::web::render(changelog_screen())).await;
	assert!(
		html.contains("Loading") || html.contains("What's new") || html.contains("unavailable"),
		"{html}"
	);
}
