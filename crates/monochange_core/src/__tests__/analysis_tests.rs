use super::*;
use crate::BumpSeverity;
use crate::PackageRecord;
use crate::PublishState;

#[test]
fn package_snapshot_file_lookup_finds_matching_paths() {
	let snapshot = PackageSnapshot {
		label: "after".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("src/lib.rs"),
			contents: "pub fn greet() {}".to_string(),
		}],
	};

	let file = snapshot
		.file(Path::new("src/lib.rs"))
		.unwrap_or_else(|| panic!("expected file in snapshot"));
	assert_eq!(file.contents, "pub fn greet() {}");
}

#[test]
fn package_analysis_context_exposes_package_root() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/repo/crates/core/Cargo.toml"),
		PathBuf::from("/repo"),
		None,
		PublishState::Public,
	);
	let context = PackageAnalysisContext {
		repo_root: Path::new("/repo"),
		package: &package,
		detection_level: DetectionLevel::Signature,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: None,
	};

	assert_eq!(context.package_root(), Path::new("/repo/crates/core"));
}

#[test]
fn api_snapshot_sorts_items_for_stable_json_output() {
	let snapshot = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new("function", "crate::z", Some("pub fn z()".to_string())),
			ApiItem::new("function", "crate::a", Some("pub fn a()".to_string())),
		],
		Vec::new(),
	);

	let json = serde_json::to_string_pretty(&snapshot)
		.unwrap_or_else(|error| panic!("serialize snapshot: {error}"));

	assert!(json.contains("\"schemaVersion\": 1"));
	assert!(
		json.find("function:crate::a") < json.find("function:crate::z"),
		"items should be sorted by stable id: {json}"
	);
}

#[test]
fn api_snapshot_diff_classifies_removed_added_and_modified_items() {
	let before = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::removed",
				Some("pub fn removed()".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed()".to_string()),
			),
		],
		Vec::new(),
	);
	let after = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed(value: u8)".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::added",
				Some("pub fn added()".to_string()),
			),
		],
		Vec::new(),
	);

	let diff = before.diff(&after);

	assert_eq!(diff.suggested_bump, BumpSeverity::Major);
	assert_eq!(diff.changes.len(), 3);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.kind == ApiChangeKind::Removed)
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.kind == ApiChangeKind::Added)
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.kind == ApiChangeKind::Modified)
	);
}

#[test]
fn diff_api_snapshots_reports_added_removed_modified_and_warnings() {
	let before = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::removed",
				Some("pub fn removed()".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed()".to_string()),
			),
			ApiItem::new("function", "crate::same", Some("pub fn same()".to_string())),
		],
		vec!["before warning".to_string()],
	);
	let after = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::added",
				Some("pub fn added()".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed(value: u8)".to_string()),
			),
			ApiItem::new("function", "crate::same", Some("pub fn same()".to_string())),
		],
		vec!["after warning".to_string()],
	);

	let diff = diff_api_snapshots(&before, &after);

	assert_eq!(diff.suggested_bump, BumpSeverity::Major);
	assert_eq!(diff.changes.len(), 3);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.summary.contains("added"))
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.summary.contains("removed"))
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.summary.contains("changed"))
	);
	assert_eq!(diff.warnings, vec!["before warning", "after warning"]);
}
