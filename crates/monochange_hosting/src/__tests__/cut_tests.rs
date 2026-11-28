use std::collections::BTreeMap;

use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleasePullRequestStrategy;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;

use super::*;

fn sample_manifest(targets: Vec<ReleaseManifestTarget>, version: Option<&str>) -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: version.map(String::from),
		group_version: version.map(String::from),
		release_targets: targets,
		released_packages: vec!["core".to_string()],
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		changesets: Vec::new(),
		deleted_changesets: Vec::new(),
		package_publications: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: std::path::PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		values: BTreeMap::new(),
		labels: BTreeMap::new(),
		label_inputs: monochange_core::versioning::LabelInputs::default(),
	}
}

fn target(id: &str, version: &str, format: VersionFormat, tag_name: &str) -> ReleaseManifestTarget {
	ReleaseManifestTarget {
		id: id.to_string(),
		kind: ReleaseOwnerKind::Group,
		version: version.to_string(),
		version_format: format,
		tag: true,
		release: true,
		tag_name: tag_name.to_string(),
		rendered_title: format!("{id} {version}"),
		rendered_changelog_title: version.to_string(),
		members: vec![id.to_string()],
		floating_tags: Vec::new(),
	}
}

fn cut_source(branch_template: &str) -> SourceConfiguration {
	let mut pull_requests = ProviderMergeRequestSettings {
		strategy: ReleasePullRequestStrategy::Cut,
		..Default::default()
	};
	pull_requests.cut.branch_template = branch_template.to_string();
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings::default(),
		pull_requests,
	}
}

#[test]
fn resolve_cut_id_prefers_primary_tag_then_single_target_then_batch_hash() {
	let primary = sample_manifest(
		vec![target("main", "0.22.0", VersionFormat::Primary, "v0.22.0")],
		Some("0.22.0"),
	);
	assert_eq!(resolve_cut_id(&primary), "v0.22.0");

	let single_namespaced = sample_manifest(
		vec![target(
			"cli",
			"1.4.0",
			VersionFormat::Namespaced,
			"cli/v1.4.0",
		)],
		None,
	);
	assert_eq!(resolve_cut_id(&single_namespaced), "cli-v1.4.0");

	let multi = sample_manifest(
		vec![
			target("b", "2.0.0", VersionFormat::Namespaced, "b/v2.0.0"),
			target("a", "1.0.0", VersionFormat::Namespaced, "a/v1.0.0"),
		],
		None,
	);
	let multi_reordered = sample_manifest(
		vec![
			target("a", "1.0.0", VersionFormat::Namespaced, "a/v1.0.0"),
			target("b", "2.0.0", VersionFormat::Namespaced, "b/v2.0.0"),
		],
		None,
	);
	assert!(resolve_cut_id(&multi).starts_with("batch-"));
	assert_eq!(resolve_cut_id(&multi), resolve_cut_id(&multi_reordered));
}

#[test]
fn resolve_cut_branches_supports_versioned_and_static_templates() {
	let manifest = sample_manifest(
		vec![target("main", "0.22.0", VersionFormat::Primary, "v0.22.0")],
		Some("0.22.0"),
	);

	let versioned = resolve_cut_branches(&cut_source("release/{{ cut_id }}"), &manifest);
	assert_eq!(versioned.cut_id, "v0.22.0");
	assert_eq!(versioned.source_base_branch, "main");
	assert_eq!(versioned.cut_base_branch, "release/v0.22.0");
	assert_eq!(versioned.head_branch, "monochange/release/v0.22.0");

	let minor_line =
		resolve_cut_branches(&cut_source("release/v{{ major }}.{{ minor }}"), &manifest);
	assert_eq!(minor_line.cut_base_branch, "release/v0.22");
	assert_eq!(minor_line.head_branch, "monochange/release/v0.22.0");

	let static_cut = resolve_cut_branches(&cut_source("release/next"), &manifest);
	assert_eq!(static_cut.cut_base_branch, "release/next");
	assert_eq!(static_cut.head_branch, "monochange/release/next");
}

#[test]
fn cut_metadata_and_status_banner_round_trip_and_replace_idempotently() {
	let metadata = CutMetadata {
		cut_id: "v0.22.0".to_string(),
		base_branch: "main".to_string(),
		cut_base_branch: "release/v0.22.0".to_string(),
		head_branch: "monochange/release/v0.22.0".to_string(),
		cut_base_commit: "1111111222222233333334444444555555556666".to_string(),
		release_commit: Some("aaaaaaa1111111".to_string()),
		targets: vec!["main".to_string()],
	};
	let initial_body = upsert_cut_metadata_comment("# Release notes", &metadata);
	assert_eq!(
		parse_cut_metadata_comment(&initial_body),
		Some(metadata.clone())
	);

	let fresh_banner = render_cut_status_banner(&CutStatusBanner {
		cut_id: "v0.22.0".to_string(),
		cut_base_commit: metadata.cut_base_commit.clone(),
		base_branch: "main".to_string(),
		commits_behind: 0,
		pending_changesets: Vec::new(),
		refresh_command: "/refresh".to_string(),
	});
	let with_fresh = upsert_cut_status_banner(&initial_body, &fresh_banner);
	assert!(with_fresh.contains("up to date with `main`"));

	let stale_banner = render_cut_status_banner(&CutStatusBanner {
		cut_id: "v0.22.0".to_string(),
		cut_base_commit: metadata.cut_base_commit.clone(),
		base_branch: "main".to_string(),
		commits_behind: 12,
		pending_changesets: (0..12)
			.map(|index| format!(".changeset/change-{index}.md"))
			.collect(),
		refresh_command: "/refresh".to_string(),
	});
	let updated = upsert_cut_status_banner(&with_fresh, &stale_banner);
	assert!(updated.contains("**12 newer commits** (12 changesets)"));
	assert!(updated.contains("comment `/refresh`"));
	assert!(updated.contains("and 2 more"));
	assert_eq!(updated.matches(CUT_STATUS_START).count(), 1);
	assert_eq!(parse_cut_metadata_comment(&updated), Some(metadata));
}

#[test]
fn render_cut_or_sync_title_expands_placeholders() {
	assert_eq!(
		render_cut_or_sync_title(
			"chore(release): sync {{ cut_id }} to {{ base }}",
			"v0.22.0",
			"main",
			Some("0.22.0")
		),
		"chore(release): sync v0.22.0 to main"
	);
}
