//! Prepared releases insert their changelog section above the previous release.
//!
//! Each fixture holds a changelog written by an earlier release. Preparing the
//! next release must place the new section directly above the newest existing
//! release heading, whatever prefix the configured changelog version title
//! renders, so every changelog reads newest first.

use std::fs;
use std::path::Path;
use std::process::Command;

use insta::assert_snapshot;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::setup_fixture;
use rstest::rstest;

fn prepare_release(root: &Path) {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env_remove("RUST_LOG")
		.args(["step", "prepare-release", "--format", "json"])
		.output()
		.unwrap_or_else(|error| panic!("run monochange step prepare-release: {error}"));

	assert!(
		output.status.success(),
		"monochange step prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

#[rstest]
#[case::primary_title("primary-title", "crates/core/changelog.md")]
#[case::namespaced_group_title_with_source("namespaced-group-title-with-source", "changelog.md")]
#[case::namespaced_package_title_without_source(
	"namespaced-package-title-without-source",
	"crates/core/changelog.md"
)]
#[case::custom_title("custom-title", "crates/core/changelog.md")]
#[case::unreleased_heading("unreleased-heading", "crates/core/changelog.md")]
#[case::no_release_headings("no-release-headings", "crates/core/changelog.md")]
fn prepare_release_inserts_section_above_previous_release(
	#[case] scenario: &str,
	#[case] changelog_path: &str,
) {
	let fixture = setup_fixture!(&format!("changelog-release-order/{scenario}"));
	let root = fixture.path();

	prepare_release(root);

	let changelog = fs::read_to_string(root.join(changelog_path))
		.unwrap_or_else(|error| panic!("read {changelog_path}: {error}"));

	insta::with_settings!({ snapshot_suffix => scenario }, {
		assert_snapshot!(changelog);
	});
}
