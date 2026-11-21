use super::*;
use crate::BumpSeverity;

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
