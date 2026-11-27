#![forbid(clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use monochange_core::VersionFormat;

use super::*;

fn sample_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![],
		package_publications: vec![],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		changesets: vec![],
		deleted_changesets: vec![],
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![],
			groups: vec![],
			warnings: vec![],
			unresolved_items: vec![],
			compatibility_evidence: vec![],
		},
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	}
}

fn minimal_target(id: &str) -> ReleaseManifestTarget {
	ReleaseManifestTarget {
		id: id.to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "0.1.0".to_string(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		tag_name: "v0.1.0".to_string(),
		members: vec![],
		rendered_title: String::new(),
		rendered_changelog_title: String::new(),
		floating_tags: Vec::new(),
	}
}

#[test]
fn oversized_headers_and_summary_pointers_still_obey_the_cap() {
	let mut manifest = sample_manifest();
	manifest.release_targets = vec![minimal_target(&"target".repeat(20_000))];
	for style in [
		ProviderPullRequestBodyStyle::Full,
		ProviderPullRequestBodyStyle::Summary,
	] {
		let result = render_release_pull_request_body(&manifest, style, Some(65_536));
		assert!(result.body.chars().count() <= 65_536);
		assert!(result.truncation.is_some());
		assert_eq!(
			result.body,
			"and more... See the changelog files in this PR for full details."
		);
	}
}
