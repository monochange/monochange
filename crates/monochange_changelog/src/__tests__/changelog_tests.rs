use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogSectionDef;
use monochange_core::ChangelogSettings;
use monochange_core::ChangelogStyle;
use monochange_core::ChangelogTarget;
use monochange_core::ChangelogType;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetRevision;
use monochange_core::ChangesetTargetKind;
use monochange_core::Ecosystem;
use monochange_core::GroupChangelogInclude;
use monochange_core::GroupDefinition;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MetadataStyle;
use monochange_core::PackageDefinition;
use monochange_core::PackageLabelStyle;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::PlannedVersionGroup;
use monochange_core::PreparedChangeset;
use monochange_core::PreparedChangesetTarget;
use monochange_core::PrereleaseConfiguration;
use monochange_core::PublishState;
use monochange_core::ReleaseDecision;
use monochange_core::VersionFormat;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use semver::Version;
use tempfile::tempdir;

use super::*;

fn empty_configuration(root: &Path) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		prerelease: PrereleaseConfiguration::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
		swift: monochange_core::EcosystemSettings::default(),
		cocoapods: monochange_core::EcosystemSettings::default(),
		gradle: monochange_core::EcosystemSettings::default(),
		version_schemes: BTreeMap::new(),
	}
}

fn sample_package_record(root: &Path, config_id: &str, name: &str) -> PackageRecord {
	let manifest_dir = root.join("packages").join(config_id);
	fs::create_dir_all(&manifest_dir)
		.unwrap_or_else(|error| panic!("create manifest dir: {error}"));
	let manifest_path = manifest_dir.join("package.json");
	fs::write(&manifest_path, "{}\n")
		.unwrap_or_else(|error| panic!("write manifest file: {error}"));

	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		name,
		manifest_path,
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), config_id.to_string());
	package
}

fn sample_package_definition(config_id: &str) -> PackageDefinition {
	PackageDefinition {
		id: config_id.to_string(),
		path: PathBuf::from(format!("packages/{config_id}")),
		package_type: PackageType::Npm,
		changelog: Some(ChangelogTarget {
			path: PathBuf::from(format!("packages/{config_id}/CHANGELOG.md")),
			format: ChangelogFormat::Monochange,
			initial_header: None,
		}),
		excluded_changelog_types: Vec::new(),
		bump_propagation: None,
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		publish: monochange_core::PublishSettings::default(),
		version_format: VersionFormat::Namespaced,
		version_source: monochange_core::VersionSource::Manifest,
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
		cli: None,
		display_version: None,
		values: BTreeMap::new(),
	}
}

fn sample_group_definition(include: GroupChangelogInclude) -> GroupDefinition {
	GroupDefinition {
		id: "sdk".to_string(),
		packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
		package_max_bumps: BTreeMap::new(),
		bump_propagation: None,
		changelog: Some(ChangelogTarget {
			path: PathBuf::from("groups/sdk/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
			initial_header: None,
		}),
		changelog_include: include,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		version_source: monochange_core::VersionSource::Manifest,
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
	}
}

fn sample_decision(package_id: &str, group_id: Option<&str>) -> ReleaseDecision {
	ReleaseDecision {
		package_id: package_id.to_string(),
		trigger_type: "changeset".to_string(),
		recommended_bump: BumpSeverity::Patch,
		planned_version: Some(Version::new(1, 2, 3)),
		group_id: group_id.map(ToString::to_string),
		reasons: vec!["covered".to_string()],
		upstream_sources: Vec::new(),
		warnings: Vec::new(),
	}
}

fn sample_group(member_ids: Vec<String>) -> PlannedVersionGroup {
	PlannedVersionGroup {
		group_id: "sdk".to_string(),
		display_name: "SDK".to_string(),
		members: member_ids,
		mismatch_detected: false,
		planned_version: Some(Version::new(2, 0, 0)),
		recommended_bump: BumpSeverity::Minor,
	}
}

fn sample_change(package_id: &str, package_name: &str, source_path: &str) -> ReleaseNoteChange {
	ReleaseNoteChange {
		package_id: package_id.to_string(),
		package_name: package_name.to_string(),
		package_labels: Vec::new(),
		source_path: Some(source_path.to_string()),
		summary: "Added release note support".to_string(),
		details: Some("Detailed explanation".to_string()),
		bump: BumpSeverity::Minor,
		change_type: Some("note".to_string()),
		stream: monochange_core::DEFAULT_CHANGELOG_STREAM.to_string(),
		context: Some("> _Owner:_ @octocat".to_string()),
		changeset_path: Some(source_path.to_string()),
		change_owner: Some("@octocat".to_string()),
		change_owner_link: Some("[@octocat](https://example.com/octocat)".to_string()),
		review_request: Some("PR 42".to_string()),
		review_request_link: Some("[PR 42](https://example.com/pr/42)".to_string()),
		introduced_commit: Some("abc1234".to_string()),
		introduced_commit_link: Some("[`abc1234`](https://example.com/commit/abc1234)".to_string()),
		last_updated_commit: Some("def5678".to_string()),
		last_updated_commit_link: Some(
			"[`def5678`](https://example.com/commit/def5678)".to_string(),
		),
		related_issues: Some("#10".to_string()),
		related_issue_links: Some("[#10](https://example.com/issues/10)".to_string()),
		closed_issues: Some("#20".to_string()),
		closed_issue_links: Some("[#20](https://example.com/issues/20)".to_string()),
	}
}

#[test]
fn changelog_file_helpers_append_and_deduplicate_updates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let changelog_path = tempdir.path().join("CHANGELOG.md");

	let first =
		append_changelog_section(&changelog_path, "## 1.0.0\n- initial", Some("# Changelog"))
			.unwrap_or_else(|error| panic!("append first changelog section: {error}"));
	assert_eq!(first, "# Changelog\n\n## 1.0.0\n- initial\n");

	let without_header_path = tempdir.path().join("without-header.md");
	let without_header =
		append_changelog_section(&without_header_path, "## 1.0.0\n- initial", None)
			.unwrap_or_else(|error| panic!("append changelog section without header: {error}"));
	assert_eq!(without_header, "## 1.0.0\n- initial\n");

	fs::write(&changelog_path, "# Changelog\n\n## 0.9.0\n- older\n")
		.unwrap_or_else(|error| panic!("write existing changelog: {error}"));
	let appended =
		append_changelog_section(&changelog_path, "## 1.0.0\n- latest", Some("# Ignored"))
			.unwrap_or_else(|error| panic!("append second changelog section: {error}"));
	assert_eq!(
		appended,
		"# Changelog\n\n## 1.0.0\n- latest\n\n## 0.9.0\n- older\n"
	);

	let earlier = ChangelogUpdate {
		file: FileUpdate {
			path: changelog_path.clone(),
			content: b"old".to_vec(),
		},
		owner_id: "pkg-a".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		output: "default".to_string(),
		stream: "default".to_string(),
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "0.9.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: "old".to_string(),
	};
	let latest = ChangelogUpdate {
		file: FileUpdate {
			path: changelog_path.clone(),
			content: b"new".to_vec(),
		},
		owner_id: "pkg-a".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		output: "default".to_string(),
		stream: "default".to_string(),
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "1.0.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: "new".to_string(),
	};
	let unique_path = tempdir.path().join("OTHER.md");
	let unique = ChangelogUpdate {
		file: FileUpdate {
			path: unique_path.clone(),
			content: b"other".to_vec(),
		},
		owner_id: "pkg-b".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		output: "default".to_string(),
		stream: "default".to_string(),
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "1.0.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: "other".to_string(),
	};
	let mut collision = latest.clone();
	collision.output = "user".to_string();
	let collision_error = validate_changelog_update_paths(&[latest.clone(), collision])
		.err()
		.unwrap_or_else(|| panic!("expected changelog output collision"));
	assert!(
		collision_error
			.to_string()
			.contains("changelog outputs `default` and `user` both render")
	);

	let deduped = dedup_changelog_updates(vec![earlier, latest.clone(), unique]);
	assert_eq!(deduped.len(), 2);
	assert!(deduped.iter().any(|update| update.file.path == unique_path));
	assert!(deduped.iter().any(|update| {
		update.file.path == changelog_path && update.file.content == latest.file.content
	}));
}

#[test]
fn named_output_builders_skip_ineligible_targets_and_report_bad_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let package = sample_package_record(root, "pkg-a", "pkg-a");
	let package_id = package.id.clone();
	let packages = vec![package];
	let package_definition = sample_package_definition("pkg-a");
	let group_definition = sample_group_definition(GroupChangelogInclude::All);
	let mut configuration = empty_configuration(root);

	configuration.packages = vec![package_definition.clone()];
	configuration.groups = vec![group_definition.clone()];

	let output = ChangelogOutputDefinition {
		stream: "user".to_string(),
		path: "{{ audience }}/notes.md".to_string(),
		format: ChangelogFormat::Monochange,
		mode: ChangelogOutputMode::Release,
		targets: vec!["pkg-a".to_string()],
		initial_header: None,
	};
	let empty_targets = (BTreeMap::new(), BTreeMap::new());
	let empty_changes = BTreeMap::new();
	let target_changes = BTreeMap::from([(
		package_id.clone(),
		vec![sample_change(&package_id, "pkg-a", ".changeset/user.md")],
	)]);
	let changeset_targets = BTreeMap::from([(
		PathBuf::from(".changeset/user.md"),
		vec![PreparedChangesetTarget {
			id: "sdk".to_string(),
			kind: ChangesetTargetKind::Group,
			bump: Some(BumpSeverity::Minor),
			origin: "manual".to_string(),
			evidence_refs: Vec::new(),
			change_type: Some("note".to_string()),
			caused_by: Vec::new(),
		}],
	)]);

	let empty_plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: Vec::new(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let context = ChangelogBuildContext {
		root,
		configuration: &configuration,
		packages: &packages,
		plan: &empty_plan,
		change_signals: &[],
		changesets: &[],
		changelog_targets: &empty_targets,
		release_targets: &[],
	};
	assert!(
		build_named_package_changelog_update(
			context,
			"user",
			&output,
			&package_definition,
			&target_changes,
			&BTreeMap::new(),
		)
		.unwrap_or_else(|error| panic!("skip missing decision: {error}"))
		.is_none()
	);

	let mut decision_without_version = sample_decision(&package_id, None);
	decision_without_version.planned_version = None;
	let plan_without_version = ReleasePlan {
		decisions: vec![decision_without_version],
		..empty_plan.clone()
	};
	let context_without_version = ChangelogBuildContext {
		plan: &plan_without_version,
		..context
	};
	assert!(
		build_named_package_changelog_update(
			context_without_version,
			"user",
			&output,
			&package_definition,
			&target_changes,
			&BTreeMap::new(),
		)
		.unwrap_or_else(|error| panic!("skip missing version: {error}"))
		.is_none()
	);

	let release_plan = ReleasePlan {
		decisions: vec![sample_decision(&package_id, None)],
		..empty_plan.clone()
	};
	let release_context = ChangelogBuildContext {
		plan: &release_plan,
		..context
	};
	assert!(
		build_named_package_changelog_update(
			release_context,
			"user",
			&output,
			&package_definition,
			&empty_changes,
			&BTreeMap::new(),
		)
		.unwrap_or_else(|error| panic!("skip empty stream: {error}"))
		.is_none()
	);
	let path_error = build_named_package_changelog_update(
		release_context,
		"user",
		&output,
		&package_definition,
		&target_changes,
		&BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid package output path"));
	assert!(
		path_error
			.to_string()
			.contains("unsupported template variable")
	);

	let context_without_package = ChangelogBuildContext {
		packages: &[],
		..release_context
	};
	assert!(
		build_named_package_changelog_update(
			context_without_package,
			"user",
			&output,
			&package_definition,
			&target_changes,
			&BTreeMap::new(),
		)
		.unwrap_or_else(|error| panic!("skip missing package: {error}"))
		.is_none()
	);

	assert!(
		build_named_group_changelog_update(
			context,
			"user",
			&output,
			&group_definition,
			&target_changes,
			&changeset_targets,
		)
		.unwrap_or_else(|error| panic!("skip missing group plan: {error}"))
		.is_none()
	);
	let mut group_without_version = sample_group(vec![package_id.clone()]);
	group_without_version.planned_version = None;
	let plan_without_group_version = ReleasePlan {
		groups: vec![group_without_version],
		..empty_plan.clone()
	};
	let context_without_group_version = ChangelogBuildContext {
		plan: &plan_without_group_version,
		..context
	};
	assert!(
		build_named_group_changelog_update(
			context_without_group_version,
			"user",
			&output,
			&group_definition,
			&target_changes,
			&changeset_targets,
		)
		.unwrap_or_else(|error| panic!("skip missing group version: {error}"))
		.is_none()
	);

	let group_plan = ReleasePlan {
		groups: vec![sample_group(vec![package_id])],
		..empty_plan.clone()
	};
	let group_context = ChangelogBuildContext {
		plan: &group_plan,
		..context
	};
	assert!(
		build_named_group_changelog_update(
			group_context,
			"user",
			&output,
			&group_definition,
			&empty_changes,
			&changeset_targets,
		)
		.unwrap_or_else(|error| panic!("skip empty group stream: {error}"))
		.is_none()
	);
	let group_path_error = build_named_group_changelog_update(
		group_context,
		"user",
		&output,
		&group_definition,
		&target_changes,
		&changeset_targets,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid group output path"));
	assert!(
		group_path_error
			.to_string()
			.contains("unsupported template variable")
	);

	let mut unknown_configuration = configuration.clone();
	unknown_configuration.changelog.outputs.insert(
		"unknown".to_string(),
		ChangelogOutputDefinition {
			targets: vec!["missing".to_string()],
			..output
		},
	);
	let unknown_target_context = ChangelogBuildContext {
		configuration: &unknown_configuration,
		..group_context
	};
	let unknown_target_updates =
		build_named_changelog_updates(unknown_target_context, &target_changes, &changeset_targets)
			.unwrap_or_else(|error| panic!("skip unknown configured target: {error}"));
	assert!(unknown_target_updates.is_empty());
}

#[test]
fn render_helpers_cover_actor_labels_links_sections_and_templates() {
	assert_eq!(
		render_actor_label(&HostedActorRef {
			login: Some("octocat".to_string()),
			source: HostedActorSourceKind::CommitAuthor,
			..HostedActorRef::default()
		}),
		"@octocat"
	);
	assert_eq!(
		render_actor_label(&HostedActorRef {
			display_name: Some("Release Bot".to_string()),
			source: HostedActorSourceKind::CommitAuthor,
			..HostedActorRef::default()
		}),
		"Release Bot"
	);
	assert_eq!(
		render_actor_label(&HostedActorRef {
			source: HostedActorSourceKind::CommitAuthor,
			..HostedActorRef::default()
		}),
		"unknown"
	);
	assert_eq!(
		render_review_request_label(&HostedReviewRequestRef {
			kind: HostedReviewRequestKind::PullRequest,
			id: "12".to_string(),
			..HostedReviewRequestRef::default()
		}),
		"PR 12"
	);
	assert_eq!(
		render_review_request_label(&HostedReviewRequestRef {
			kind: HostedReviewRequestKind::MergeRequest,
			id: "9".to_string(),
			..HostedReviewRequestRef::default()
		}),
		"MR 9"
	);
	assert_eq!(render_markdown_link("plain", None), "plain");
	assert_eq!(
		render_markdown_link("linked", Some("https://example.com")),
		"[linked](https://example.com)"
	);
	let issues = [
		HostedIssueRef {
			id: "#1".to_string(),
			url: Some("https://example.com/issues/1".to_string()),
			relationship: HostedIssueRelationshipKind::Mentioned,
			..HostedIssueRef::default()
		},
		HostedIssueRef {
			id: "#2".to_string(),
			url: None,
			relationship: HostedIssueRelationshipKind::Manual,
			..HostedIssueRef::default()
		},
	];

	let issue_refs = issues.iter().collect::<Vec<_>>();
	assert_eq!(render_issue_labels(&issue_refs), "#1, #2");
	assert_eq!(
		render_issue_links(&issue_refs),
		"[#1](https://example.com/issues/1), #2"
	);

	let mut multi_label_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
	multi_label_change.package_labels = vec!["pkg-a".to_string(), "pkg-b".to_string()];
	let multi_label_entry = release_notes_entry(&multi_label_change, "sdk");
	let block = format_structured_labeled_entry(
		&multi_label_entry,
		"#### Summary\n\nMore",
		&ChangelogStyle {
			package_label_style: PackageLabelStyle::Badge,
			..ChangelogStyle::default()
		},
	);
	assert!(
		block.contains("_Packages:_ 🟠 *pkg-a*, 🟠 *pkg-b*"),
		"block was: {block}"
	);
	assert_eq!(
		format_structured_labeled_entry(
			&multi_label_entry,
			"#### Summary\n\nMore",
			&ChangelogStyle {
				package_label_style: PackageLabelStyle::Badge,
				..ChangelogStyle::default()
			}
		),
		"#### Summary\n_Packages:_ 🟠 *pkg-a*, 🟠 *pkg-b*\n\nMore"
	);

	let mut single_label_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
	single_label_change.package_labels = vec!["pkg-a".to_string()];
	let single_label_entry = release_notes_entry(&single_label_change, "sdk");
	assert_eq!(
		format_structured_labeled_entry(
			&single_label_entry,
			"- Added release note support",
			&ChangelogStyle::default()
		),
		"- Added release note support\n_Packages:_ 🟠 _pkg-a_"
	);
	assert_eq!(
		format_structured_labeled_entry(
			&single_label_entry,
			"- Added release note support",
			&ChangelogStyle {
				package_label_style: PackageLabelStyle::Badge,
				..ChangelogStyle::default()
			}
		),
		"- Added release note support\n_Packages:_ 🟠 *pkg-a*"
	);
	assert_eq!(
		format_structured_labeled_entry(
			&single_label_entry,
			"- Added release note support",
			&ChangelogStyle {
				package_label_style: PackageLabelStyle::Omit,
				..ChangelogStyle::default()
			}
		),
		"- Added release note support"
	);
	assert_eq!(
		format_metadata_line(MetadataStyle::Blockquote, "Owner: @octocat"),
		"> Owner: @octocat"
	);
	assert_eq!(
		format_metadata_line(MetadataStyle::Plain, "Owner: @octocat"),
		"Owner: @octocat"
	);

	assert_eq!(
		format_metadata_line(MetadataStyle::Omit, "Owner: @octocat"),
		""
	);
	assert_eq!(
		format_metadata_line(MetadataStyle::Inline, "Owner: @octocat"),
		"Owner: @octocat"
	);

	// Inline metadata style renders context as single paragraph joined with ' · '
	assert!(
		ChangelogStyle::default()
			.rules()
			.contains("single inline paragraph joined with")
	);

	let mut empty_summary_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
	empty_summary_change.summary.clear();
	empty_summary_change.details = None;
	empty_summary_change.context = None;
	let mut empty_summary_entry = release_notes_entry(&empty_summary_change, "pkg-a");
	empty_summary_entry.provenance = ReleaseNoteProvenance::default();
	assert_eq!(
		render_configured_release_note_entry(
			&empty_summary_entry,
			&ChangelogStyle::default(),
			&[],
			"pkg-a",
			"1.2.3",
		),
		"- "
	);
	assert_eq!(
		render_configured_release_note_entry(
			&empty_summary_entry,
			&ChangelogStyle::default(),
			&["{{ missing }}".to_string()],
			"pkg-a",
			"1.2.3",
		),
		"- "
	);

	let sample_entry =
		release_notes_entry(&sample_change("pkg-a", "pkg-a", ".changeset/a.md"), "sdk");
	let omit_style = ChangelogStyle {
		metadata_style: MetadataStyle::Omit,
		..ChangelogStyle::default()
	};
	let blockquote_style = ChangelogStyle {
		metadata_style: MetadataStyle::Blockquote,
		..ChangelogStyle::default()
	};
	let plain_style = ChangelogStyle {
		metadata_style: MetadataStyle::Plain,
		..ChangelogStyle::default()
	};
	assert_eq!(render_release_note_context(&sample_entry, &omit_style), "");
	assert!(render_release_note_context(&sample_entry, &blockquote_style).contains("> _Owner:_"));
	assert!(render_release_note_context(&sample_entry, &plain_style).contains("\n_Review:_"));

	let rendered = apply_release_note_entry_template(
		"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}\n\n{{ change_owner_link }}\n\n{{ review_request_link }}\n\n{{ introduced_commit_link }}\n\n{{ last_updated_commit_link }}\n\n{{ related_issue_links }}\n\n{{ closed_issue_links }}",
		&sample_entry,
		&ChangelogStyle::default(),
		"sdk",
		"1.2.3",
	)
	.unwrap_or_else(|| panic!("expected template to render"));
	assert!(rendered.contains("Detailed explanation"));
	assert!(rendered.contains("[@octocat](https://example.com/octocat)"));
	assert!(rendered.contains("[PR 42](https://example.com/pr/42)"));
	assert!(rendered.contains("[#10](https://example.com/issues/10)"));
	assert!(rendered.contains("[#20](https://example.com/issues/20)"));
	assert!(
		apply_release_note_entry_template(
			"{{ missing_value }}",
			&sample_entry,
			&ChangelogStyle::default(),
			"sdk",
			"1.2.3"
		)
		.is_none()
	);
	assert!(
		apply_release_note_entry_template(
			"   ",
			&sample_entry,
			&ChangelogStyle::default(),
			"sdk",
			"1.2.3"
		)
		.is_none()
	);

	let mut extra_settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	extra_settings.sections.insert(
		"highlights".to_string(),
		ChangelogSectionDef {
			heading: "Highlights".to_string(),
			description: None,
			priority: 20,
		},
	);
	extra_settings.sections.insert(
		"notes".to_string(),
		ChangelogSectionDef {
			heading: "Notes".to_string(),
			description: None,
			priority: 100,
		},
	);
	extra_settings.types.insert(
		"minor".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "highlights".to_string(),
			description: None,
		},
	);
	extra_settings.types.insert(
		"note".to_string(),
		ChangelogType {
			bump: BumpSeverity::None,
			section: "notes".to_string(),
			description: None,
		},
	);
	let sections = build_release_note_sections(
		"sdk",
		&extra_settings,
		&[
			ReleaseNoteChange {
				change_type: Some("note".to_string()),
				..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
			},
			ReleaseNoteChange {
				change_type: Some("minor".to_string()),
				bump: BumpSeverity::Minor,
				summary: "Added group support".to_string(),
				..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
			},
			ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Major,
				summary: "Breaking API".to_string(),
				..sample_change("pkg-c", "pkg-c", ".changeset/c.md")
			},
			ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Patch,
				summary: "Bug fix".to_string(),
				..sample_change("pkg-d", "pkg-d", ".changeset/d.md")
			},
			ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Patch,
				summary: "Bug fix".to_string(),
				..sample_change("pkg-d", "pkg-d", ".changeset/d.md")
			},
		],
	);
	assert_eq!(sections[0].title, "Highlights");
	assert_eq!(sections[1].title, "Notes");

	assert_eq!(sections[2].title, "Changed");
	assert_eq!(sections[0].entries[0].summary, "Added group support");
	assert_eq!(sections[2].entries[0].summary, "Breaking API");
	assert_eq!(sections[2].entries[1].summary, "Bug fix");

	let fallback = build_release_note_sections("sdk", &ChangelogSettings::default(), &[]);
	assert_eq!(fallback[0].title, "Changed");
	assert_eq!(fallback[0].entries[0].summary, "Prepare release");

	let document = build_release_notes_document(
		"sdk",
		"1.2.3",
		vec!["Summary".to_string()],
		&extra_settings,
		&[sample_change("pkg-a", "pkg-a", ".changeset/a.md")],
	);
	assert_eq!(document.title, "1.2.3");
	assert_eq!(document.summary, vec!["Summary".to_string()]);
	assert!(!document.sections.is_empty());
}

#[test]
fn snapshot_metadata_styles_render_context_blocks() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let changeset_path = tempdir.path().join(".changeset/snapshot-feature.md");
	fs::create_dir_all(changeset_path.parent().unwrap_or_else(|| Path::new(".")))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::write(&changeset_path, "feature\n")
		.unwrap_or_else(|error| panic!("write changeset file: {error}"));

	// Changeset with all metadata: owner, review request, commits, issues.
	let changeset = PreparedChangeset {
		path: changeset_path.clone(),
		summary: Some("snapshot feature".to_string()),
		details: Some("snapshot details".to_string()),
		targets: vec![PreparedChangesetTarget {
			id: "pkg-a".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: Some(BumpSeverity::Minor),
			origin: "manual".to_string(),
			evidence_refs: Vec::new(),
			change_type: Some("note".to_string()),
			caused_by: Vec::new(),
		}],
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GitHub,
			host: Some("github.com".to_string()),
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: Some("1".to_string()),
					login: Some("octocat".to_string()),
					display_name: Some("Octo Cat".to_string()),
					url: Some("https://example.com/octocat".to_string()),
					source: HostedActorSourceKind::ReviewRequestAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					sha: "abcdef123456".to_string(),
					short_sha: "abcdef1".to_string(),
					url: Some("https://example.com/commit/abcdef1".to_string()),
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: Some(HostedReviewRequestRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					kind: HostedReviewRequestKind::PullRequest,
					id: "42".to_string(),
					title: Some("feat: add feature".to_string()),
					url: Some("https://example.com/pull/42".to_string()),
					author: None,
				}),
			}),
			last_updated: Some(ChangesetRevision {
				actor: None,
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					sha: "9876543210".to_string(),
					short_sha: "9876543".to_string(),
					url: Some("https://example.com/commit/9876543".to_string()),
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: None,
			}),
			related_issues: vec![
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: "123".to_string(),
					relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
					title: Some("Bug fix".to_string()),
					url: Some("https://example.com/issues/123".to_string()),
				},
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: "456".to_string(),
					relationship: HostedIssueRelationshipKind::ReferencedByReviewRequest,
					title: None,
					url: None,
				},
			],
		}),
	};

	// Snapshot each metadata style
	let blockquote =
		build_rendered_changeset_context(tempdir.path(), &changeset, MetadataStyle::Blockquote);
	insta::assert_snapshot!("metadata_style_blockquote", blockquote.context);

	let plain = build_rendered_changeset_context(tempdir.path(), &changeset, MetadataStyle::Plain);
	insta::assert_snapshot!("metadata_style_plain", plain.context);

	let inline =
		build_rendered_changeset_context(tempdir.path(), &changeset, MetadataStyle::Inline);
	insta::assert_snapshot!("metadata_style_inline", inline.context);

	let omit = build_rendered_changeset_context(tempdir.path(), &changeset, MetadataStyle::Omit);
	insta::assert_snapshot!("metadata_style_omit", omit.context);

	// Inline style without a PR should include commit links
	let mut changeset_no_pr = changeset.clone();
	// Remove the review request from the introduced revision to test commit inclusion
	if let Some(ref mut context) = changeset_no_pr.context
		&& let Some(ref mut introduced) = context.introduced
	{
		introduced.review_request = None;
	}
	let inline_no_pr =
		build_rendered_changeset_context(tempdir.path(), &changeset_no_pr, MetadataStyle::Inline);
	insta::assert_snapshot!("metadata_style_inline_no_pr", inline_no_pr.context);
}
