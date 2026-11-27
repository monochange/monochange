use std::fs;

use super::*;

#[test]
fn release_record_migration_reports_end_with_the_migration_outcome() {
	let record = |path: &str, status: ReleaseRecordMigrationStatus| {
		ReleaseRecordMigrationEntry {
			path: path.to_string(),
			status,
			from_schema_version: "1".to_string(),
			to_schema_version: "2".to_string(),
		}
	};
	let mut report = ReleaseRecordMigrationReport {
		dry_run: true,
		current_schema_version: "2".to_string(),
		scanned: 2,
		migrated: 1,
		unchanged: 1,
		records: vec![
			record(
				".monochange/releases/a/release.json",
				ReleaseRecordMigrationStatus::WouldMigrate,
			),
			record(
				".monochange/releases/b/release.json",
				ReleaseRecordMigrationStatus::Current,
			),
		],
	};
	insta::assert_snapshot!(
		"release_record_migration_dry_run",
		text_release_record_migration_report(&report)
	);

	report.dry_run = false;
	report.records[0].status = ReleaseRecordMigrationStatus::Migrated;
	assert!(
		text_release_record_migration_report(&report)
			.lines()
			.last()
			.unwrap_or_default()
			.starts_with("✔ Migrated 1 of 2 release records to schema 2")
	);

	report.migrated = 0;
	report.records.clear();
	assert_eq!(
		text_release_record_migration_report(&report),
		"✔ Release records already use schema 2 · 2 release records checked"
	);
}
