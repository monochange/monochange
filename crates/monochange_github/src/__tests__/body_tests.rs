use super::*;

#[test]
fn boundaries_ignore_code_fences_and_nested_headings() {
	let body = "intro\n## Features\n### One\n```md\n### fake\n~~~\n```\n  ## nested\n### Two\n~~~~\n## fake\n~~~\n~~~~\n- entry\n";
	let prefixes = entry_boundaries(body)
		.into_iter()
		.map(|end| body.get(..end).unwrap_or_default())
		.collect::<Vec<_>>();
	insta::assert_debug_snapshot!(prefixes);
	assert_eq!(entry_boundaries(""), [] as [usize; 0]);
}

#[test]
fn release_cap_keeps_metadata_and_links_to_tagged_changelog() {
	let mut source = crate::tests::sample_source(None);
	let manifest = crate::tests::sample_manifest();
	let target = manifest.release_targets.first().unwrap();
	let body = format!(
		"## Features\n### One\n_Packages:_ core\n\n### Two\n{}",
		"more\n".repeat(30_000)
	);
	let bounded = bounded_release_body(&source, &manifest, target, &body);
	assert!(bounded.chars().count() <= 125_000);
	insta::assert_snapshot!(bounded);
	source.releases.max_body_chars = Some(usize::MAX);
	assert_eq!(
		bounded_release_body(&source, &manifest, target, &body),
		bounded
	);
	source.releases.max_body_chars = Some(20);
	assert_eq!(
		bounded_release_body(&source, &manifest, target, &body)
			.chars()
			.count(),
		20
	);
	assert_eq!(
		bounded_release_body(&source, &manifest, target, "fits"),
		"fits"
	);
}

#[test]
fn request_builders_apply_the_provider_limits() {
	let mut source = crate::tests::sample_source(None);
	let mut manifest = crate::tests::sample_manifest();
	let changelog = manifest.changelogs.first_mut().unwrap();
	changelog.rendered = format!(
		"## 1.2.0\n\n### Features\n\n#### Huge entry\n{}",
		"x".repeat(130_000)
	);
	let requests = crate::build_release_requests(&source, &manifest);
	let body = requests.first().unwrap().body.as_ref().unwrap();
	assert!(body.chars().count() < 125_000);
	insta::assert_snapshot!(body);
	source.pull_requests.max_body_chars = Some(usize::MAX);
	manifest.release_targets.first_mut().unwrap().members = vec!["m".repeat(70_000)];
	let request = crate::build_release_pull_request_request(&source, &manifest);
	assert!(request.body.chars().count() <= 65_536);
}

#[test]
fn changelog_pointer_handles_missing_outputs_and_escaped_paths() {
	let mut source = crate::tests::sample_source(None);
	source.host = Some("https://github.example/".to_owned());
	source.releases.max_body_chars = Some(300);
	let mut manifest = crate::tests::sample_manifest();
	let body = "é".repeat(1000);
	let mut target = manifest.release_targets.first().unwrap().clone();
	target.tag_name = "core/v1.2.0".to_owned();
	manifest.changelogs.first_mut().unwrap().path = "notes/full details.md".into();
	insta::assert_snapshot!(
		"escaped_path",
		bounded_release_body(&source, &manifest, &target, &body)
	);
	manifest.changelogs.clear();
	insta::assert_snapshot!(
		"missing_output",
		bounded_release_body(&source, &manifest, &target, &body)
	);
}
