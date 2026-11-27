mod prop_tests;

use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::DependencyEdge;
use monochange_core::DependencyKind;
use monochange_core::DependencySourceKind;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::VersionGroup;
use semver::Version;

use crate::NormalizedGraph;
use crate::build_release_plan;

fn package(id: &str, version: Version) -> PackageRecord {
	let manifest_path = PathBuf::from(id.replace(':', "/")).join("manifest");
	let mut package = PackageRecord::new(
		Ecosystem::Cargo,
		id.to_string(),
		manifest_path,
		PathBuf::from("fixtures/mixed"),
		Some(version),
		PublishState::Public,
	);
	package.id = id.to_string();
	package
}

fn edge(from: &str, to: &str) -> DependencyEdge {
	DependencyEdge {
		from_package_id: from.to_string(),
		to_package_id: to.to_string(),
		dependency_kind: DependencyKind::Runtime,
		source_kind: DependencySourceKind::Manifest,
		version_constraint: None,
		is_optional: false,
		is_direct: true,
		source_field: None,
	}
}

#[test]
fn transitive_dependents_handles_cycles_without_looping_forever() {
	let packages = [
		package("a", Version::new(1, 0, 0)),
		package("b", Version::new(1, 0, 0)),
	];
	let edges = [edge("a", "b"), edge("b", "a")];
	let graph = NormalizedGraph::new(&packages, &edges);

	let dependents = graph.transitive_dependents("a");
	assert!(dependents.contains("b"));
	assert_eq!(dependents.len(), 2);
}

#[test]
fn build_release_plan_propagates_explicit_member_versions_to_group_version() {
	let mut core = package("cargo:core", Version::new(1, 0, 0));
	core.version_group_id = Some("sdk".to_string());
	let mut web = package("npm:web", Version::new(1, 0, 0));
	web.version_group_id = Some("sdk".to_string());
	let version_group = VersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![core.id.clone(), web.id.clone()],
		member_max_bumps: BTreeMap::new(),
		mismatch_detected: false,
	};
	let plan = build_release_plan(
		PathBuf::from("fixtures/mixed").as_path(),
		&[core.clone(), web.clone()],
		&[],
		&[version_group],
		&[ChangeSignal {
			package_id: core.id.clone(),
			requested_bump: Some(BumpSeverity::Major),
			explicit_version: Some(Version::new(2, 0, 0)),
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("promote sdk".to_string()),
			details: None,
			change_type: None,
			caused_by: Vec::new(),
			source_path: PathBuf::from(".changeset/group-pin.md"),
		}],
		&[],
		BumpSeverity::Patch,
		&BTreeMap::new(),
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let group = plan
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected one group"));
	assert_eq!(group.planned_version, Some(Version::new(2, 0, 0)));
	assert!(
		plan.decisions
			.iter()
			.all(|decision| decision.planned_version == Some(Version::new(2, 0, 0)))
	);
}

#[test]
fn build_release_plan_uses_highest_conflicting_explicit_version_with_warning() {
	let packages = vec![package("cargo:core", Version::new(1, 0, 0))];
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[],
		&[],
		&[
			ChangeSignal {
				package_id: "cargo:core".to_string(),
				requested_bump: Some(BumpSeverity::Minor),
				explicit_version: Some(Version::new(1, 1, 0)),
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("first pin".to_string()),
				details: None,
				change_type: None,
				caused_by: Vec::new(),
				source_path: PathBuf::from(".changeset/001-first.md"),
			},
			ChangeSignal {
				package_id: "cargo:core".to_string(),
				requested_bump: Some(BumpSeverity::Major),
				explicit_version: Some(Version::new(2, 0, 0)),
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("second pin".to_string()),
				details: None,
				change_type: None,
				caused_by: Vec::new(),
				source_path: PathBuf::from(".changeset/002-second.md"),
			},
		],
		&[],
		BumpSeverity::Patch,
		&BTreeMap::new(),
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let decision = plan
		.decisions
		.first()
		.unwrap_or_else(|| panic!("expected one decision"));
	let warning = plan
		.warnings
		.first()
		.unwrap_or_else(|| panic!("expected one warning"));
	assert_eq!(decision.planned_version, Some(Version::new(2, 0, 0)));
	assert_eq!(plan.warnings.len(), 1);
	insta::assert_snapshot!(
		"build_release_plan_conflicting_explicit_versions_warning",
		warning
	);
}

#[test]
fn build_release_plan_suppresses_matching_dependency_propagation_when_caused_by_references_group() {
	let mut core = package("cargo:core", Version::new(1, 0, 0));
	core.version_group_id = Some("sdk".to_string());
	let mut util = package("cargo:util", Version::new(1, 0, 0));
	util.version_group_id = Some("sdk".to_string());
	let packages = vec![
		core.clone(),
		util.clone(),
		package("cargo:app", Version::new(1, 0, 0)),
	];
	let version_group = VersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![core.id.clone(), util.id.clone()],
		member_max_bumps: BTreeMap::new(),
		mismatch_detected: false,
	};
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[edge("cargo:app", "cargo:core")],
		&[version_group],
		&[
			ChangeSignal {
				package_id: core.id.clone(),
				requested_bump: Some(BumpSeverity::Minor),
				explicit_version: None,
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("core feature".to_string()),
				details: None,
				change_type: None,
				caused_by: Vec::new(),
				source_path: PathBuf::from(".changeset/core.md"),
			},
			ChangeSignal {
				package_id: "cargo:app".to_string(),
				requested_bump: Some(BumpSeverity::None),
				explicit_version: None,
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("dependency-only follow-up".to_string()),
				details: None,
				change_type: None,
				caused_by: vec!["sdk".to_string()],
				source_path: PathBuf::from(".changeset/app.md"),
			},
		],
		&[],
		BumpSeverity::Patch,
		&BTreeMap::new(),
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let app = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:app")
		.unwrap_or_else(|| panic!("expected app decision"));
	assert_eq!(app.recommended_bump, BumpSeverity::None);
	assert_eq!(app.trigger_type, "direct-change");
	assert_eq!(
		app.upstream_sources,
		vec!["cargo:core".to_string(), "cargo:util".to_string()]
	);
}
