#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use serde_json::json;

use crate::BumpSeverity;
use crate::ChangelogFormat;
use crate::ChangelogStyle;
use crate::CliStepDefinition;
use crate::CollapsedSectionStyle;
use crate::Ecosystem;
use crate::HostedSourceAdapter;
use crate::HostedSourceFeatures;
use crate::HostingProviderKind;
use crate::MetadataStyle;
use crate::PackageLabelStyle;
use crate::PackageRecord;
use crate::ProviderMergeRequestSettings;
use crate::ProviderReleaseSettings;
use crate::PublishFlow;
use crate::PublishMode;
use crate::RELEASE_RECORD_END_MARKER;
use crate::RELEASE_RECORD_HEADING;
use crate::RELEASE_RECORD_KIND;
use crate::RELEASE_RECORD_SCHEMA_VERSION;
use crate::RELEASE_RECORD_START_MARKER;
use crate::RegistryKind;
use crate::ReleaseManifest;
use crate::ReleaseManifestPlan;
use crate::ReleaseNoteEntryStyle;
use crate::ReleaseNotePackage;
use crate::ReleaseNoteProvenance;
use crate::ReleaseNoteReference;
use crate::ReleaseNotesDocument;
use crate::ReleaseNotesEntry;
use crate::ReleaseNotesSection;
use crate::ReleaseOwnerKind;
use crate::ReleaseRecord;
use crate::ReleaseRecordProvider;
use crate::ReleaseRecordTarget;
use crate::SectionSeparator;
use crate::ShellConfig;
use crate::SourceConfiguration;
use crate::SourceProvider;
use crate::VersionFormat;
use crate::default_publish_order_dependency_fields;
use crate::render_release_notes;

struct DefaultHostedSourceAdapter {
	provider: SourceProvider,
}

impl HostedSourceAdapter for DefaultHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		self.provider
	}

	fn annotate_changeset_context(
		&self,
		_source: &SourceConfiguration,
		changesets: &mut [crate::PreparedChangeset],
	) {
		if let Some(first) = changesets.first_mut() {
			first.summary = Some("annotated".to_string());
		}
	}
}

struct PanicDiscoveryAdapter;

impl crate::EcosystemAdapter for PanicDiscoveryAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Cargo
	}

	fn discover(&self, _root: &Path) -> crate::MonochangeResult<crate::AdapterDiscovery> {
		panic!("discovery panic for coverage");
	}

	fn load_configured(
		&self,
		_root: &Path,
		_package_path: &Path,
	) -> crate::MonochangeResult<Option<PackageRecord>> {
		Ok(None)
	}

	fn supported_versioned_file_kind(&self, _path: &Path) -> bool {
		false
	}

	fn validate_versioned_file(
		&self,
		_full_path: &Path,
		_display_path: &str,
		_custom_fields: Option<&[String]>,
	) -> crate::MonochangeResult<()> {
		Ok(())
	}
}

fn test_source_configuration(provider: SourceProvider) -> SourceConfiguration {
	SourceConfiguration {
		provider,
		owner: "org".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn test_release_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: true,
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: Vec::new(),
		released_packages: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		package_publications: Vec::new(),
		changesets: Vec::new(),
		deleted_changesets: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		label_inputs: crate::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	}
}

#[test]
fn publish_mode_and_registry_kind_display_canonical_names() {
	assert_eq!(PublishMode::Builtin.as_str(), "builtin");
	assert_eq!(PublishMode::External.as_str(), "external");
	assert_eq!(PublishMode::Builtin.to_string(), "builtin");
	assert_eq!(PublishMode::External.to_string(), "external");

	assert_eq!(PublishFlow::Direct.as_str(), "direct");
	assert_eq!(PublishFlow::Staged.as_str(), "staged");
	assert_eq!(PublishFlow::Direct.to_string(), "direct");
	assert_eq!(PublishFlow::Staged.to_string(), "staged");
	assert!(PublishFlow::Direct.is_default());
	assert!(!PublishFlow::Staged.is_default());

	assert_eq!(RegistryKind::CratesIo.as_str(), "crates_io");
	assert_eq!(RegistryKind::Npm.as_str(), "npm");
	assert_eq!(RegistryKind::Jsr.as_str(), "jsr");
	assert_eq!(RegistryKind::PubDev.as_str(), "pub_dev");
	assert_eq!(RegistryKind::Pypi.as_str(), "pypi");
	assert_eq!(RegistryKind::GoProxy.as_str(), "go_proxy");
	assert_eq!(RegistryKind::GoProxy.to_string(), "go_proxy");
}

#[test]
fn hosted_source_adapter_default_features_are_empty_and_comment_plans_are_empty() {
	let adapter = DefaultHostedSourceAdapter {
		provider: SourceProvider::GitLab,
	};
	let source = test_source_configuration(SourceProvider::GitLab);
	let manifest = test_release_manifest();

	assert_eq!(adapter.features(), HostedSourceFeatures::default());
	assert!(
		adapter
			.plan_released_issue_comments(&source, &manifest)
			.is_empty()
	);
}

#[test]
fn hosted_source_adapter_default_release_planning_methods_are_noops() {
	let adapter = DefaultHostedSourceAdapter {
		provider: SourceProvider::GitLab,
	};
	let source = test_source_configuration(SourceProvider::GitLab);
	let manifest = test_release_manifest();

	assert!(adapter.tag_url(&source, "v1.2.3").is_empty());
	assert!(adapter.compare_url(&source, "v1.2.2", "v1.2.3").is_empty());
	assert!(
		adapter
			.build_release_requests(&source, &manifest)
			.is_empty()
	);
	assert!(
		std::panic::catch_unwind(|| {
			adapter.build_release_pull_request_request(&source, &manifest);
		})
		.is_err()
	);
}

#[test]
fn ecosystem_registry_gracefully_handles_panicked_discovery_worker() {
	let registry = crate::EcosystemRegistry::new().with_adapter(Box::new(PanicDiscoveryAdapter));
	let result = registry
		.discover_all(Path::new("."))
		.expect("discovery should succeed even with panicking adapter");

	assert!(
		result.packages.is_empty(),
		"panicked adapter should produce no packages"
	);
	assert!(
		result.warnings.iter().any(|w| w.contains("panicked")),
		"should include a warning about the panicked worker, got: {:#?}",
		result.warnings
	);
}

#[test]
fn always_run_accessor_returns_value_for_all_variants() {
	let validate = CliStepDefinition::Validate {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(validate.always_run());

	let validate_false = CliStepDefinition::Validate {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!validate_false.always_run());

	let command = CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: None,
		always_run: true,
		command: String::new(),
		dry_run_command: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(command.always_run());

	let publish = CliStepDefinition::PublishPackages {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(publish.always_run());

	let prepare = CliStepDefinition::PrepareRelease {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
		allow_empty_changesets: false,
	};
	assert!(!prepare.always_run());

	let config = CliStepDefinition::Config {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!config.always_run());

	let discover = CliStepDefinition::Discover {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(discover.always_run());

	let display = CliStepDefinition::DisplayVersions {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!display.always_run());

	let change = CliStepDefinition::CreateChangeFile {
		show_progress: None,
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(change.always_run());

	let commit = CliStepDefinition::CommitRelease {
		name: None,
		when: None,
		always_run: false,
		commit_backend: crate::ReleaseBackend::default(),
		hosted_auth: crate::HostedCommitAuth::default(),
		hosted_url: None,
		oidc_audience: None,
		inputs: BTreeMap::new(),
		no_verify: false,
		update_release_json: false,
		stage_all: false,
	};
	assert!(!commit.always_run());

	let verify = CliStepDefinition::VerifyReleaseBranch {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(verify.always_run());

	let publish_release = CliStepDefinition::PublishRelease {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!publish_release.always_run());

	let placeholder = CliStepDefinition::PlaceholderPublish {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(placeholder.always_run());

	let plan_limits = CliStepDefinition::PlanPublishRateLimits {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!plan_limits.always_run());

	let open_request = CliStepDefinition::OpenReleaseRequest {
		name: None,
		when: None,
		always_run: true,
		no_verify: false,
		stage_all: false,
		backend: crate::ReleaseBackend::default(),
		hosted_auth: crate::HostedCommitAuth::default(),
		hosted_url: None,
		inputs: BTreeMap::new(),
	};
	assert!(open_request.always_run());

	let comment = CliStepDefinition::CommentReleasedIssues {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!comment.always_run());

	let affected = CliStepDefinition::AffectedPackages {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(affected.always_run());

	let diagnose = CliStepDefinition::DiagnoseChangesets {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(!diagnose.always_run());

	let retarget = CliStepDefinition::RetargetRelease {
		name: None,
		when: None,
		always_run: true,
		inputs: BTreeMap::new(),
	};
	assert!(retarget.always_run());
}

#[test]
fn hosting_provider_kind_as_str_and_display_cover_all_variants() {
	assert_eq!(HostingProviderKind::GenericGit.as_str(), "generic_git");
	assert_eq!(HostingProviderKind::GitHub.as_str(), "github");
	assert_eq!(HostingProviderKind::GitLab.as_str(), "gitlab");
	assert_eq!(HostingProviderKind::Gitea.as_str(), "gitea");
	assert_eq!(HostingProviderKind::Forgejo.as_str(), "forgejo");
	assert_eq!(HostingProviderKind::Bitbucket.as_str(), "bitbucket");
	assert_eq!(HostingProviderKind::Forgejo.to_string(), "forgejo");
}

#[test]
fn hosted_issue_relationship_kind_as_str_and_display() {
	use crate::HostedIssueRelationshipKind;
	let cases = [
		(
			HostedIssueRelationshipKind::ClosedByReviewRequest,
			"closed_by_review_request",
		),
		(
			HostedIssueRelationshipKind::ReferencedByReviewRequest,
			"referenced_by_review_request",
		),
		(HostedIssueRelationshipKind::Mentioned, "mentioned"),
		(HostedIssueRelationshipKind::Manual, "manual"),
	];

	for (kind, expected) in cases {
		assert_eq!(kind.as_str(), expected);
		assert_eq!(kind.to_string(), expected);
	}
}

#[test]
fn render_release_notes_supports_markdown_json_and_text_formats() {
	let _snapshot = insta::Settings::clone_current().bind_to_scope();
	let document = ReleaseNotesDocument {
		title: "1.2.3".to_string(),
		summary: vec!["Grouped release for `sdk`.".to_string()],
		sections: vec![
			ReleaseNotesSection {
				title: "Changed".to_string(),
				collapsed: false,
				entries: vec!["#### add release automation\n\n- faster releases".to_string()],
			},
			ReleaseNotesSection {
				title: "Empty".to_string(),
				collapsed: false,
				entries: Vec::new(),
			},
		],
	};

	let monochange = render_release_notes(
		ChangelogFormat::Monochange,
		&document,
		&ChangelogStyle::default(),
	);
	let keep_a_changelog = render_release_notes(
		ChangelogFormat::KeepAChangelog,
		&document,
		&ChangelogStyle::default(),
	);
	let json = render_release_notes(ChangelogFormat::Json, &document, &ChangelogStyle::default());
	let text = render_release_notes(ChangelogFormat::Text, &document, &ChangelogStyle::default());

	insta::assert_snapshot!(
		"render_release_notes_supports_monochange_and_keep_a_changelog_formats__monochange",
		monochange
	);
	insta::assert_snapshot!(
		"render_release_notes_supports_monochange_and_keep_a_changelog_formats__keep_a_changelog",
		keep_a_changelog
	);
	let mut json_value = serde_json::from_str::<serde_json::Value>(&json)
		.unwrap_or_else(|error| panic!("parse rendered JSON release notes: {error}"));
	json_value["sections"][0]["entries"][0] = json!("[multiline text]");
	insta::assert_json_snapshot!("render_release_notes_supports_formats__json", json_value);
	insta::assert_snapshot!(
		"render_release_notes_supports_formats__json_entry",
		&document.sections[0].entries[0]
	);
	insta::assert_snapshot!("render_release_notes_supports_formats__text", text);
}

#[test]
fn structured_release_notes_render_data_without_markdown_leaking_into_text() {
	let document = ReleaseNotesDocument {
		title: "1.2.3".to_string(),
		summary: Vec::new(),
		sections: vec![ReleaseNotesSection {
			title: "Fixes".to_string(),
			collapsed: false,
			entries: vec![
				ReleaseNotesEntry {
					summary: "Keep JSON failures non-zero".to_string(),
					details_markdown: Some(
						"`monochange check` now returns **status 1**.".to_string(),
					),
					packages: Vec::new(),
					change_type: Some("fix".to_string()),
					bump: BumpSeverity::Patch,
					stream: "default".to_string(),
					style: ReleaseNoteEntryStyle::Compact,
					provenance: ReleaseNoteProvenance::default(),
				},
				ReleaseNotesEntry {
					summary: "Replace the release-note document".to_string(),
					details_markdown: Some("Migrate callers to the typed entry API.".to_string()),
					packages: vec![ReleaseNotePackage::new(
						"monochange_core",
						BumpSeverity::Patch,
					)],
					change_type: Some("breaking".to_string()),
					bump: BumpSeverity::Major,
					stream: "default".to_string(),
					style: ReleaseNoteEntryStyle::Expanded,
					provenance: ReleaseNoteProvenance {
						review_request: Some(ReleaseNoteReference {
							label: "PR 842".to_string(),
							url: Some("https://example.com/pull/842".to_string()),
						}),
						..ReleaseNoteProvenance::default()
					},
				},
			],
		}],
	};

	let markdown = crate::render_structured_release_notes(
		ChangelogFormat::Monochange,
		&document,
		&ChangelogStyle::default(),
	);
	assert!(markdown.contains("- **Keep JSON failures non-zero.**"));
	assert!(markdown.contains("#### Replace the release-note document"));
	assert!(markdown.contains("_Packages:_ 🟢 _monochange_core_"));

	let text = crate::render_structured_release_notes(
		ChangelogFormat::Text,
		&document,
		&ChangelogStyle::default(),
	);
	assert!(!text.contains("**"));
	assert!(!text.contains('`'));
	assert!(text.contains("monochange check now returns status 1."));
	assert_eq!(
		crate::plain_markdown("Keep _important_ `package_name` values"),
		"Keep important package_name values"
	);

	let json = crate::render_structured_release_notes(
		ChangelogFormat::Json,
		&document,
		&ChangelogStyle::default(),
	);
	let json: serde_json::Value =
		serde_json::from_str(&json).unwrap_or_else(|error| panic!("structured JSON: {error}"));
	assert_eq!(
		json["sections"][0]["entries"][0]["summary"],
		"Keep JSON failures non-zero"
	);
	assert_eq!(json["sections"][0]["entries"][1]["style"], "expanded");
}

#[test]
fn structured_release_notes_cover_layout_and_metadata_variants() {
	let provenance = ReleaseNoteProvenance {
		source_path: Some(".changeset/fix.md".to_string()),
		changeset_path: Some(".changeset/fix.md".to_string()),
		change_owner: Some(ReleaseNoteReference {
			label: "@octocat".to_string(),
			url: None,
		}),
		review_request: None,
		introduced_commit: Some(ReleaseNoteReference {
			label: "abc1234".to_string(),
			url: Some("https://example.com/commit/abc1234".to_string()),
		}),
		last_updated_commit: Some(ReleaseNoteReference {
			label: "def5678".to_string(),
			url: None,
		}),
		closed_issues: vec![
			ReleaseNoteReference {
				label: "#20".to_string(),
				url: Some("https://example.com/issues/20".to_string()),
			},
			ReleaseNoteReference {
				label: "#21".to_string(),
				url: None,
			},
		],
		related_issues: vec![ReleaseNoteReference {
			label: "#10".to_string(),
			url: None,
		}],
	};
	let expanded_entry = ReleaseNotesEntry {
		summary: "Migrate the release API".to_string(),
		details_markdown: Some(
			"Use [`new_api`](https://example.com/new).\n\n```rust\nnew_api();\n```".to_string(),
		),
		packages: vec![
			ReleaseNotePackage::new("core", BumpSeverity::Patch),
			ReleaseNotePackage::new("cli", BumpSeverity::Patch),
		],
		change_type: Some("breaking".to_string()),
		bump: BumpSeverity::Major,
		stream: "default".to_string(),
		style: ReleaseNoteEntryStyle::Expanded,
		provenance,
	};
	let compact_entry = ReleaseNotesEntry {
		summary: "Tighten output?".to_string(),
		details_markdown: Some("Keep `snake_case` names.".to_string()),
		packages: vec![ReleaseNotePackage::new("cli", BumpSeverity::Patch)],
		change_type: Some("fix".to_string()),
		bump: BumpSeverity::Patch,
		stream: "default".to_string(),
		style: ReleaseNoteEntryStyle::Compact,
		provenance: ReleaseNoteProvenance::default(),
	};
	let document = ReleaseNotesDocument {
		title: "[`sdk`](https://example.com/sdk) 2.0.0".to_string(),
		summary: vec![
			"First **summary**.".to_string(),
			"Second summary.".to_string(),
		],
		sections: vec![
			ReleaseNotesSection {
				title: "Empty".to_string(),
				collapsed: false,
				entries: Vec::new(),
			},
			ReleaseNotesSection {
				title: "Breaking changes".to_string(),
				collapsed: true,
				entries: vec![expanded_entry.clone(), expanded_entry.clone()],
			},
			ReleaseNotesSection {
				title: "Fixes".to_string(),
				collapsed: false,
				entries: vec![compact_entry.clone()],
			},
		],
	};

	let blockquote_style = ChangelogStyle {
		section_separator: SectionSeparator::ThematicBreak,
		package_label_style: PackageLabelStyle::Badge,
		metadata_style: MetadataStyle::Blockquote,
		collapsed_section_style: CollapsedSectionStyle::Details,
		package_bump_symbols: true,
	};
	let markdown = crate::render_structured_release_notes(
		ChangelogFormat::KeepAChangelog,
		&document,
		&blockquote_style,
	);
	assert!(markdown.contains("<summary><strong>Breaking changes</strong></summary>"));
	assert!(markdown.contains("_Packages:_ 🟢 *core*, 🟢 *cli*"));
	assert!(markdown.contains("> _Last updated in:_ def5678"));
	assert!(markdown.contains("> _Closed issues:_ [#20](https://example.com/issues/20), #21"));
	assert!(markdown.contains("\n---\n"));
	let no_separator = crate::render_structured_release_notes(
		ChangelogFormat::Monochange,
		&document,
		&ChangelogStyle {
			section_separator: SectionSeparator::None,
			collapsed_section_style: CollapsedSectionStyle::Plain,
			..ChangelogStyle::default()
		},
	);

	assert!(!no_separator.contains("\n---\n"));

	let legacy = document.to_legacy_markdown(&blockquote_style);
	assert_eq!(legacy.title, document.title);
	assert_eq!(legacy.sections[1].entries.len(), 2);

	let plain_style = ChangelogStyle {
		metadata_style: MetadataStyle::Plain,
		collapsed_section_style: CollapsedSectionStyle::Plain,
		..ChangelogStyle::default()
	};
	let expanded = crate::render_release_note_entry_markdown(&expanded_entry, &plain_style);
	assert!(expanded.contains("_Packages:_ 🟢 _core_, 🟢 _cli_"));
	assert!(expanded.contains("_Related issues:_ #10"));
	let compact = crate::render_release_note_entry_markdown(&compact_entry, &plain_style);
	assert!(compact.contains("\n  _Packages:_ 🟢 _cli_"));

	let omitted = crate::render_release_note_entry_markdown(
		&expanded_entry,
		&ChangelogStyle {
			metadata_style: MetadataStyle::Omit,
			..ChangelogStyle::default()
		},
	);
	assert!(!omitted.contains("Owner:"));

	let mut text_document = document.clone();
	text_document.sections[2].entries.push(ReleaseNotesEntry {
		details_markdown: None,
		..compact_entry.clone()
	});
	let text = crate::render_structured_release_notes(
		ChangelogFormat::Text,
		&text_document,
		&ChangelogStyle::default(),
	);
	assert!(text.contains("sdk 2.0.0\n\nFirst summary.\n\nSecond summary."));
	assert!(text.contains("Last updated in: def5678"));
	assert!(text.contains("Closed issues: #20, #21"));
	assert!(text.contains("Related issues: #10"));
	assert!(!text.contains("```"));

	assert_eq!(crate::plain_markdown_inline("[broken"), "[broken");
	assert_eq!(
		crate::plain_markdown_inline("[label](missing"),
		"[label](missing"
	);
	assert_eq!(crate::strip_underscore_emphasis("_open"), "_open");
	assert_eq!(
		crate::strip_underscore_emphasis("_value_more"),
		"_value_more"
	);
}

#[test]
fn render_release_notes_applies_separator_and_plain_collapsed_styles() {
	let document = ReleaseNotesDocument {
		title: "1.2.3".to_string(),
		summary: vec![
			"First paragraph.".to_string(),
			"Second paragraph.".to_string(),
		],
		sections: vec![
			ReleaseNotesSection {
				title: "Features".to_string(),
				collapsed: false,
				entries: vec![
					"adds dashboards".to_string(),
					"- preserves list markers".to_string(),
				],
			},
			ReleaseNotesSection {
				title: "Documentation".to_string(),
				collapsed: true,
				entries: vec![
					"first line\nsecond line".to_string(),
					"wrap plain entry".to_string(),
				],
			},
		],
	};
	let style = ChangelogStyle {
		section_separator: SectionSeparator::ThematicBreak,
		package_label_style: PackageLabelStyle::Inline,
		metadata_style: MetadataStyle::Plain,
		collapsed_section_style: CollapsedSectionStyle::Plain,
		package_bump_symbols: true,
	};

	let monochange = render_release_notes(ChangelogFormat::Monochange, &document, &style);
	let keep_a_changelog = render_release_notes(ChangelogFormat::KeepAChangelog, &document, &style);

	for rendered in [monochange, keep_a_changelog] {
		assert!(rendered.contains("First paragraph.\n\nSecond paragraph."));
		assert!(rendered.contains("### Features\n\n- adds dashboards\n- preserves list markers"));
		assert!(rendered.contains(
			"\n---\n\n### Documentation\n\nfirst line\nsecond line\n\n- wrap plain entry"
		));
		assert!(!rendered.contains("<details>"));
	}

	let compact_style = ChangelogStyle {
		section_separator: SectionSeparator::None,
		package_label_style: PackageLabelStyle::Inline,
		metadata_style: MetadataStyle::Plain,
		collapsed_section_style: CollapsedSectionStyle::Details,
		package_bump_symbols: true,
	};

	for compact_format in [ChangelogFormat::Monochange, ChangelogFormat::KeepAChangelog] {
		let compact = render_release_notes(compact_format, &document, &compact_style);
		assert!(
			compact
				.contains("### Features\n\n- adds dashboards\n- preserves list markers\n<details>")
		);
	}
}

#[test]
fn release_record_block_roundtrips_with_reserved_markers() {
	let record = sample_release_record();
	let rendered = crate::render_release_record_block(&record)
		.unwrap_or_else(|error| panic!("render release record: {error}"));

	assert!(rendered.starts_with(RELEASE_RECORD_HEADING));
	assert!(rendered.contains(RELEASE_RECORD_START_MARKER));
	assert!(rendered.contains(RELEASE_RECORD_END_MARKER));
	assert!(rendered.contains("```json"));

	let parsed = crate::parse_release_record_block(&rendered)
		.unwrap_or_else(|error| panic!("parse release record: {error}"));
	assert_eq!(parsed, record);
}

fn sample_release_record() -> ReleaseRecord {
	ReleaseRecord {
		schema_version: RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-06T12:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		versions: BTreeMap::from([("main".to_string(), "1.2.3".to_string())]),
		release_targets: vec![ReleaseRecordTarget {
			id: "main".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			rendered_title: "main v1.2.3 (2026-04-06)".to_string(),
			rendered_changelog_title: "[1.2.3](https://example.com/v1.2.3) (2026-04-06)"
				.to_string(),
			members: vec![
				"monochange".to_string(),
				"monochange_core".to_string(),
				"monochange_config".to_string(),
			],
			floating_tags: Vec::new(),
		}],
		released_packages: vec![
			"monochange".to_string(),
			"monochange_core".to_string(),
			"monochange_config".to_string(),
		],
		changed_files: vec![
			PathBuf::from("Cargo.lock"),
			PathBuf::from("crates/monochange/Cargo.toml"),
		],
		package_publications: Vec::new(),
		updated_changelogs: vec![PathBuf::from("crates/monochange/CHANGELOG.md")],
		deleted_changesets: vec![PathBuf::from(".changeset/032-step-outputs.md")],
		changesets: Vec::new(),
		changelogs: Vec::new(),
		values: BTreeMap::new(),
		labels: BTreeMap::new(),
		label_inputs: crate::versioning::LabelInputs::default(),
		provider: Some(ReleaseRecordProvider {
			kind: SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

#[test]
fn json_helper_functions_cover_error_paths() {
	let range_error = crate::apply_json_replacements(
		"{}",
		vec![(crate::JsonSpan { start: 10, end: 11 }, "\"x\"".to_string())],
	)
	.err()
	.unwrap_or_else(|| panic!("expected range error"));
	assert!(range_error.to_string().contains("out of bounds"));

	let root_error = crate::json_root_object_start("[]")
		.err()
		.unwrap_or_else(|| panic!("expected root error"));
	assert!(root_error.to_string().contains("expected JSON object"));

	let locate_error = crate::find_json_object_field_value_span("[]", 0, "name")
		.err()
		.unwrap_or_else(|| panic!("expected locate error"));
	assert!(
		locate_error
			.to_string()
			.contains("expected JSON object when locating field")
	);

	for (contents, key) in [
		("{1:2}", "a"),
		("{\"a\" 1}", "a"),
		("{\"a\":1 !}", "missing"),
		("{\"a\":1", "missing"),
	] {
		assert!(
			crate::find_json_object_field_value_span(contents, 0, key).is_err(),
			"contents: {contents}"
		);
	}

	assert!(crate::skip_json_value("", 0).is_err());
	assert!(crate::skip_json_array("[1 !]", 0).is_err());
	assert!(crate::skip_json_array("[1", 0).is_err());
	assert!(crate::skip_json_array("[", 0).is_err());
	assert!(crate::skip_json_object("{\"a\":1 !}", 0).is_err());
	assert!(crate::skip_json_object("{\"a\":1", 0).is_err());
	assert!(crate::skip_json_object("{", 0).is_err());
	assert!(crate::skip_json_object("{1}", 0).is_err());
	assert!(crate::skip_json_object("{\"a\" 1}", 0).is_err());
	assert!(crate::parse_json_string_span("abc", 0).is_err());
	assert!(crate::parse_json_string_span("\"abc", 0).is_err());
	// Truncated escape: backslash at end of input.
	let error = crate::parse_json_string_span("\"abc\\", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for truncated escape"));
	assert!(
		error.to_string().contains("unterminated escape sequence"),
		"expected truncated-escape error, got: {error}"
	);
	// Escaped quote should not close the string.
	assert!(crate::parse_json_string_span("\"abc\\\"", 0).is_err());
	// Double backslash followed by closing quote should work.
	let (span, next) = crate::parse_json_string_span("\"abc\\\\\"", 0)
		.unwrap_or_else(|error| panic!("double-backslash: {error}"));
	assert_eq!(span, crate::JsonSpan { start: 1, end: 6 });

	assert_eq!(next, 7);
	// Valid unicode escape \u0041 (letter A).
	let (span, next) = crate::parse_json_string_span("\"\\u0041\"", 0)
		.unwrap_or_else(|error| panic!("unicode escape: {error}"));
	assert_eq!(span, crate::JsonSpan { start: 1, end: 7 });
	assert_eq!(next, 8);
	// Incomplete unicode escape: fewer than 4 hex digits before input ends.
	let error = crate::parse_json_string_span("\"\\u00", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for incomplete unicode escape"));
	assert!(
		error.to_string().contains("incomplete unicode escape"),
		"got: {error}"
	);
	// Incomplete unicode escape inside a quoted string (quote seen as non-hex).
	let error = crate::parse_json_string_span("\"\\u00\"", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for short unicode escape"));
	assert!(
		error.to_string().contains("invalid unicode escape"),
		"got: {error}"
	);
	// Invalid hex digit in unicode escape.
	let error = crate::parse_json_string_span("\"\\u00ZZ\"", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for invalid hex in unicode escape"));
	assert!(
		error.to_string().contains("invalid unicode escape"),
		"got: {error}"
	);
	// Truncated unicode escape: string ends before 4 hex digits.
	let error = crate::parse_json_string_span("\"\\u00", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for truncated unicode escape"));
	assert!(
		error.to_string().contains("incomplete unicode escape"),
		"got: {error}"
	);
}

#[test]
fn json_helper_functions_cover_success_paths() {
	let (string_span, next) = crate::parse_json_string_span("\"a\\\"b\"", 0)
		.unwrap_or_else(|error| panic!("parse string span: {error}"));
	assert_eq!(string_span, crate::JsonSpan { start: 1, end: 5 });
	assert_eq!(next, 6);

	assert_eq!(
		crate::skip_json_value("\"text\"", 0)
			.unwrap_or_else(|error| panic!("skip string value: {error}")),
		6
	);
	assert_eq!(
		crate::skip_json_value("{\"a\":1}", 0)
			.unwrap_or_else(|error| panic!("skip object value: {error}")),
		7
	);
	assert_eq!(
		crate::skip_json_value("[1,2]", 0)
			.unwrap_or_else(|error| panic!("skip array value: {error}")),
		5
	);
	assert_eq!(crate::skip_json_primitive("true /* comment */", 0), 4);
	assert_eq!(crate::skip_json_primitive("true//comment", 0), 4);
	assert_eq!(
		crate::skip_json_ws_and_comments(" // comment\n /* block */ {", 0),
		25
	);
	assert_eq!(
		crate::skip_json_object("{}", 0)
			.unwrap_or_else(|error| panic!("skip empty object: {error}")),
		2
	);
	assert_eq!(
		crate::skip_json_object("{\"a\":1,\"b\":2}", 0)
			.unwrap_or_else(|error| panic!("skip object with comma: {error}")),
		13
	);

	assert_eq!(
		crate::skip_json_array("[]", 0).unwrap_or_else(|error| panic!("skip empty array: {error}")),
		2
	);
	assert_eq!(
		crate::find_json_object_field_value_span("{}", 0, "name")
			.unwrap_or_else(|error| panic!("find empty object field: {error}")),
		None
	);
	let field_span = crate::find_json_object_field_value_span(
		r#"{"name":"tool","deps":{"core":"^1.0.0"}}"#,
		0,
		"deps",
	)
	.unwrap_or_else(|error| panic!("find field span: {error}"))
	.unwrap_or_else(|| panic!("expected deps field"));
	assert!(crate::json_span_is_object(
		r#"{"name":"tool","deps":{"core":"^1.0.0"}}"#,
		field_span
	));
	let updated = crate::update_json_manifest_text(
		r#"{"version":1,"imports":{"core":{"path":"./core"}}}"#,
		Some("2.0.0"),
		&["imports"],
		&BTreeMap::from([("core".to_string(), "^2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update json manifest with non-string values: {error}"));
	assert_eq!(
		updated,
		r#"{"version":1,"imports":{"core":{"path":"./core"}}}"#
	);
}

#[test]
fn version_format_rejects_each_invalid_git_tag_shape() {
	let cases = [
		("", "tag is empty"),
		("/v1.2.3", "starts or ends with `/`"),
		("v1.2.3/", "starts or ends with `/`"),
		("-v1.2.3", "starts with `-`"),
		("pkg//v1.2.3", "empty path component"),
		("pkg/../v1.2.3", "contains `..`"),
		("pkg/@{upstream}/v1.2.3", "contains `@{`"),
		("pkg/v1.2.3.", "ends with `.`"),
		("pkg/file.lock/v1.2.3", "ends with `.lock`"),
		("pkg/file.LOCK/v1.2.3", "ends with `.lock`"),
		("pkg/v1.2.3~", "not valid in a Git ref"),
	];

	for (tag, reason) in cases {
		let error = crate::validate_version_format_tag(tag)
			.expect_err("expected invalid version format tag");
		assert!(
			error.render().contains(reason),
			"expected `{}` to contain `{reason}`",
			error.render()
		);
	}
}

#[test]
fn version_format_rejects_malformed_template_syntax() {
	let cases = [
		("   ", "must not be empty"),
		("release", "must include the `{{ version }}` variable"),
		("{{ version }}/{{ name", "unterminated template variable"),
		("{{ name }}/v{{ version }}}}", "unopened template variable"),
	];

	for (template, reason) in cases {
		let error = crate::render_version_format_template(template, "cli", "1.2.3", "cargo")
			.expect_err("expected invalid version format template");
		assert!(
			error.render().contains(reason),
			"expected `{}` to contain `{reason}`",
			error.render()
		);
	}
}

#[allow(deprecated)]
#[test]
fn native_ecosystem_conversions_and_defaults() {
	use crate::EcosystemType;
	use crate::PackageType;
	use crate::RegistryKind;
	assert_eq!(Ecosystem::from(EcosystemType::Swift), Ecosystem::Swift);
	assert_eq!(
		Ecosystem::from(EcosystemType::CocoaPods),
		Ecosystem::CocoaPods
	);
	assert_eq!(Ecosystem::from(EcosystemType::Gradle), Ecosystem::Gradle);
	assert_eq!(Ecosystem::from(PackageType::Swift), Ecosystem::Swift);
	assert_eq!(
		Ecosystem::from(PackageType::CocoaPods),
		Ecosystem::CocoaPods
	);
	assert_eq!(Ecosystem::from(PackageType::Gradle), Ecosystem::Gradle);

	assert_eq!("swift".parse::<Ecosystem>(), Ok(Ecosystem::Swift));
	assert_eq!("cocoapods".parse::<Ecosystem>(), Ok(Ecosystem::CocoaPods));
	assert_eq!("gradle".parse::<Ecosystem>(), Ok(Ecosystem::Gradle));
	assert_eq!(Ecosystem::Swift.as_str(), "swift");
	assert_eq!(Ecosystem::CocoaPods.as_str(), "cocoapods");
	assert_eq!(Ecosystem::Gradle.as_str(), "gradle");
	assert!(Ecosystem::Swift.versions_from_tags());
	assert!(!Ecosystem::CocoaPods.versions_from_tags());

	assert!(!Ecosystem::Gradle.versions_from_tags());

	assert_eq!(
		crate::default_registry_kind_for_ecosystem(Ecosystem::Swift),
		None
	);
	assert_eq!(
		crate::default_registry_kind_for_ecosystem(Ecosystem::CocoaPods),
		Some(RegistryKind::CocoaPodsTrunk)
	);
	assert_eq!(
		crate::default_registry_kind_for_ecosystem(Ecosystem::Gradle),
		Some(RegistryKind::MavenCentral)
	);
	assert_eq!(RegistryKind::CocoaPodsTrunk.as_str(), "cocoapods_trunk");
	assert_eq!(RegistryKind::MavenCentral.as_str(), "maven_central");
	assert_eq!(EcosystemType::Swift.default_fields(), &[] as &[&str]);
	assert_eq!(EcosystemType::CocoaPods.default_fields(), &[] as &[&str]);
	assert_eq!(EcosystemType::Gradle.default_fields(), &[] as &[&str]);
	assert_eq!(EcosystemType::Swift.default_prefix(), "");
	assert_eq!(
		default_publish_order_dependency_fields(Ecosystem::Swift),
		&[] as &[&str]
	);
	assert_eq!(
		default_publish_order_dependency_fields(Ecosystem::CocoaPods),
		&[] as &[&str]
	);
	assert_eq!(
		default_publish_order_dependency_fields(Ecosystem::Gradle),
		&[] as &[&str]
	);
	assert_eq!(PackageType::Swift.as_str(), "swift");

	assert_eq!(PackageType::CocoaPods.as_str(), "cocoapods");
	assert_eq!(PackageType::Gradle.as_str(), "gradle");
	assert_eq!(
		PackageType::Swift.manifest_file_name(),
		Some("Package.swift")
	);
	assert_eq!(PackageType::CocoaPods.manifest_file_name(), None);
	assert_eq!(
		PackageType::Gradle.manifest_file_name(),
		Some("gradle.properties")
	);
}
