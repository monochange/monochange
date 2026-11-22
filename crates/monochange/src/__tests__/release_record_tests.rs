#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use monochange_core::FloatingTagFormat;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::ReleaseRecordTarget;
use monochange_core::VersionFormat;
use tempfile::TempDir;

use super::*;
use crate::git_support;

fn sample_tag_report(status: &str, operations: &[ReleaseTagOperation]) -> ReleaseTagReport {
	ReleaseTagReport {
		from: "HEAD".to_string(),
		resolved_from_commit: "def5678901234".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 1,
		push: status != "dry_run",
		dry_run: status == "dry_run",
		tag_results: operations
			.iter()
			.enumerate()
			.map(|(index, operation)| {
				ReleaseTagResult {
					tag_name: format!("v1.{index}.0"),
					target_commit: "abc1234567890".to_string(),
					existing_commit: match operation {
						ReleaseTagOperation::AlreadyUpToDate => Some("abc1234567890".to_string()),
						ReleaseTagOperation::Created if index > 0 => {
							Some("0123456789abc".to_string())
						}

						_ => None,
					},

					operation: *operation,
					floating_results: if index == 0 {
						vec![
							ReleaseFloatingTagResult {
								tag_name: "v1".to_string(),
								previous_commit: Some("0123456789abc".to_string()),
							},
							ReleaseFloatingTagResult {
								tag_name: "v1.0".to_string(),
								previous_commit: None,
							},
						]
					} else {
						Vec::new()
					},
				}
			})
			.collect(),
		tags: BTreeMap::new(),
		status: status.to_string(),
	}
}

#[test]
fn release_tag_reports_end_with_what_happened_to_the_tags() {
	insta::assert_snapshot!(
		"release_tag_report_dry_run",
		text_release_tag_report(&sample_tag_report(
			"dry_run",
			&[ReleaseTagOperation::Planned]
		))
	);
	insta::assert_snapshot!(
		"release_tag_report_completed",
		text_release_tag_report(&sample_tag_report(
			"completed",
			&[
				ReleaseTagOperation::Created,
				ReleaseTagOperation::AlreadyUpToDate,
				ReleaseTagOperation::Created,
			]
		))
	);
	assert!(
		text_release_tag_report(&sample_tag_report(
			"already_up_to_date",
			&[ReleaseTagOperation::AlreadyUpToDate]
		))
		.lines()
		.last()
		.unwrap_or_default()
		.starts_with("✔ Release tags are already up to date · push enabled")
	);
	let none = text_release_tag_report(&sample_tag_report("no_tags_declared", &[]));
	assert!(
		none.lines()
			.last()
			.unwrap_or_default()
			.starts_with("• The release record declares no tags"),
		"{none}"
	);
	assert!(!none.contains("Tags ("), "{none}");
}
