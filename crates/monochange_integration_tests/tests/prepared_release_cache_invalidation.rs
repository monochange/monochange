//! Integration coverage for prepared-release cache invalidation.
//!
//! `monochange preview` is the surface a maintainer reviews before a release, so
//! the implicit `.monochange/local/prepared-release-cache.json` plan must never
//! be reused after its inputs change. Git status lines cannot see an edit to an
//! untracked or already-dirty file, so these cases edit a changeset in place and
//! change its bytes without changing its path.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Map;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/prepared-release/cache-invalidation")
		.join(relative)
}

fn setup_cache_invalidation_repo() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("workspace"), root);
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "monochange tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(
		root,
		&["-c", "commit.gpgsign=false", "commit", "-m", "initial"],
	);
	tempdir
}

/// Install a checked-in fixture payload as a changeset file.
fn install_changeset(root: &Path, payload: &str, changeset_name: &str) {
	let source = fixture_path("payloads").join(payload);
	let destination = root.join(".changeset").join(changeset_name);
	std::fs::create_dir_all(destination.parent().unwrap_or(root))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	std::fs::copy(&source, &destination).unwrap_or_else(|error| {
		panic!(
			"copy fixture payload {} to {}: {error}",
			source.display(),
			destination.display()
		)
	});
}

fn monochange_command() -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command.env("MONOCHANGE_NO_PROGRESS", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

fn run_monochange(root: &Path, args: &[&str]) -> String {
	let output = monochange_command()
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {args:?}: {error}"));

	assert!(
		output.status.success(),
		"monochange {args:?} failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"))
}

fn preview_json(root: &Path) -> Value {
	let stdout = run_monochange(root, &["preview", "--format", "json"]);
	serde_json::from_str(&stdout)
		.unwrap_or_else(|error| panic!("parse preview json: {error}\nraw:\n{stdout}"))
}

fn planned_version(json: &Value, target_id: &str) -> String {
	json["release_targets"]
		.as_array()
		.unwrap_or_else(|| panic!("release_targets was not an array: {json:#?}"))
		.iter()
		.find(|target| target["id"] == target_id)
		.unwrap_or_else(|| panic!("missing release target `{target_id}` in {json:#?}"))["version"]
		.as_str()
		.unwrap_or_else(|| panic!("release target `{target_id}` had no version: {json:#?}"))
		.to_string()
}

/// A stable projection of the preview payload: planned versions, the changeset
/// set, the changed-file set, and the rendered changelog entries.
fn preview_summary(json: &Value) -> Value {
	let mut release_targets = Map::new();
	for target in json["release_targets"]
		.as_array()
		.unwrap_or_else(|| panic!("release_targets was not an array: {json:#?}"))
	{
		let id = target["id"]
			.as_str()
			.unwrap_or_else(|| panic!("release target id: {target:#?}"));
		release_targets.insert(id.to_string(), target["version"].clone());
	}

	let changelog_entries = json["changelogs"]
		.as_array()
		.unwrap_or_else(|| panic!("changelogs was not an array: {json:#?}"))
		.iter()
		.flat_map(|changelog| {
			changelog["notes"]["sections"]
				.as_array()
				.map(|sections| {
					sections
						.iter()
						.flat_map(|section| {
							section["entries"]
								.as_array()
								.map(|entries| entries.to_vec())
								.unwrap_or_default()
						})
						.collect::<Vec<_>>()
				})
				.unwrap_or_default()
		})
		.collect::<Vec<_>>();

	serde_json::json!({
		"release_targets": release_targets,
		"released_packages": json["released_packages"],
		"changed_files": json["changed_files"],
		"changelog_entries": changelog_entries,
	})
}

fn rendered_changelog(json: &Value) -> String {
	json["changelogs"]
		.as_array()
		.unwrap_or_else(|| panic!("changelogs was not an array: {json:#?}"))
		.first()
		.and_then(|changelog| changelog["rendered"].as_str())
		.unwrap_or_else(|| panic!("missing rendered changelog: {json:#?}"))
		.to_string()
}

#[test]
fn preview_replans_when_untracked_changeset_severity_changes_in_place() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	install_changeset(root, "alpha-minor.md", "feature.md");

	let minor = preview_json(root);
	assert_eq!(planned_version(&minor, "alpha"), "1.1.0");
	assert_json_snapshot!("severity_in_place_minor", preview_summary(&minor));

	// Same path, same untracked status line, different bytes and severity.
	install_changeset(root, "alpha-major.md", "feature.md");
	let major = preview_json(root);
	assert_eq!(
		planned_version(&major, "alpha"),
		"2.0.0",
		"rewriting an untracked changeset in place must be replanned, not served from the cache\nfull payload:\n{major:#?}"
	);
	assert_json_snapshot!("severity_in_place_major", preview_summary(&major));

	// Rewriting the file back to the original bytes must plan the original
	// version again without clearing the cache by hand.
	install_changeset(root, "alpha-minor.md", "feature.md");
	let restored = preview_json(root);
	assert_eq!(
		planned_version(&restored, "alpha"),
		"1.1.0",
		"rewriting the changeset back must replan the original version\nfull payload:\n{restored:#?}"
	);
	assert_json_snapshot!("severity_in_place_restored", preview_summary(&restored));
}

#[test]
fn preview_replans_when_changeset_body_changes_without_severity_change() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	install_changeset(root, "alpha-minor.md", "feature.md");

	let original = preview_json(root);
	assert_snapshot!("body_only_original_rendered", rendered_changelog(&original));

	// Same severity, so the plan's version is identical; only the changelog text
	// can reveal that the changeset bytes changed.
	install_changeset(root, "alpha-minor-rewritten.md", "feature.md");
	let rewritten = preview_json(root);
	assert_eq!(
		planned_version(&rewritten, "alpha"),
		planned_version(&original, "alpha"),
		"fixture must keep the same severity so only the body changes"
	);
	assert_ne!(
		rendered_changelog(&original),
		rendered_changelog(&rewritten),
		"fixture must change the rendered changelog text"
	);
	assert_snapshot!(
		"body_only_rewritten_rendered",
		rendered_changelog(&rewritten)
	);
}

#[test]
fn preview_replans_when_changeset_set_changes() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	install_changeset(root, "alpha-minor.md", "alpha-minor.md");

	let alpha_only = preview_json(root);
	assert_json_snapshot!("set_alpha_only", preview_summary(&alpha_only));

	install_changeset(root, "beta-patch.md", "beta-patch.md");
	let alpha_and_beta = preview_json(root);
	assert_eq!(planned_version(&alpha_and_beta, "beta"), "1.0.1");
	assert_json_snapshot!("set_alpha_and_beta", preview_summary(&alpha_and_beta));

	std::fs::remove_file(root.join(".changeset/beta-patch.md"))
		.unwrap_or_else(|error| panic!("remove beta changeset: {error}"));
	let alpha_again = preview_json(root);
	assert_eq!(planned_version(&alpha_again, "alpha"), "1.1.0");
	assert_json_snapshot!("set_alpha_again", preview_summary(&alpha_again));
}

#[test]
fn preview_reuses_cache_when_the_workspace_is_unchanged() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	install_changeset(root, "alpha-minor.md", "feature.md");

	let first = run_monochange(root, &["preview"]);
	assert!(
		!first.contains("reused prepared release artifact"),
		"the first preview must compute the plan rather than reuse one:\n{first}"
	);
	assert_snapshot!("unchanged_first_preview", first);
	assert!(
		root.join(".monochange/local/prepared-release-cache.json")
			.is_file(),
		"preview must write the implicit prepared-release cache"
	);

	let second = run_monochange(root, &["preview"]);
	assert!(
		second.contains("reused prepared release artifact"),
		"repeated identical preview runs must keep hitting the cache:\n{second}"
	);
	assert_snapshot!("unchanged_second_preview", second);
	assert_eq!(
		first.replace(
			"  reused prepared release artifact `.monochange/local/prepared-release-cache.json`\n",
			""
		),
		second.replace(
			"  reused prepared release artifact `.monochange/local/prepared-release-cache.json`\n",
			""
		),
		"a cache hit must render the same plan as a cache miss"
	);
}

#[test]
fn explicit_prepared_release_artifact_is_honored_after_changesets_are_consumed() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let artifact = ".monochange/local/explicit-prepared-release.json";
	install_changeset(root, "alpha-minor.md", "feature.md");

	let first = run_monochange(
		root,
		&[
			"run",
			"release",
			"--format",
			"json",
			"--prepared-release",
			artifact,
		],
	);
	let first_json: Value =
		serde_json::from_str(&first).unwrap_or_else(|error| panic!("parse release json: {error}"));
	assert_eq!(planned_version(&first_json, "alpha"), "1.1.0");
	assert!(root.join(artifact).is_file());
	assert!(
		!root.join(".changeset/feature.md").exists(),
		"a non-dry-run release consumes its changesets"
	);

	// The changesets are gone, so only the explicit artifact can drive this run.
	let second = run_monochange(root, &["run", "release", "--prepared-release", artifact]);
	assert!(
		second.contains("reused prepared release artifact"),
		"an explicitly supplied artifact is a deliberate override and must still load:\n{second}"
	);
	assert_snapshot!("explicit_artifact_second_release", second);
}
