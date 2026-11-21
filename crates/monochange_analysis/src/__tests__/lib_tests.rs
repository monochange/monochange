use std::fs;

use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output_trimmed;
use tempfile::tempdir;

use super::*;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_analysis_repo(relative: &str) -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path(relative), tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	git(tempdir.path(), &["branch", "-M", "main"]);

	tempdir
}

#[test]
fn package_lifecycle_change_tracks_manifest_presence() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/repo/crates/core/Cargo.toml"),
		PathBuf::from("/repo"),
		None,
		monochange_core::PublishState::Public,
	);
	let with_manifest = PackageSnapshot {
		label: "with".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("Cargo.toml"),
			contents: "[package]".to_string(),
		}],
	};
	let without_manifest = PackageSnapshot {
		label: "without".to_string(),
		files: Vec::new(),
	};

	let removed = package_lifecycle_change(&package, &with_manifest, &without_manifest)
		.unwrap_or_else(|| panic!("expected removed package evidence"));
	assert_eq!(removed.category, SemanticChangeCategory::Package);
	assert_eq!(removed.kind, SemanticChangeKind::Removed);
	assert_eq!(
		removed.before_signature.as_deref(),
		Some("cargo package `core`")
	);
	assert!(removed.after_signature.is_none());

	let added = package_lifecycle_change(&package, &without_manifest, &with_manifest)
		.unwrap_or_else(|| panic!("expected added package evidence"));
	assert_eq!(added.kind, SemanticChangeKind::Added);
	assert!(added.before_signature.is_none());
	assert_eq!(
		added.after_signature.as_deref(),
		Some("cargo package `core`")
	);

	assert!(package_lifecycle_change(&package, &with_manifest, &with_manifest).is_none());
	assert!(package_lifecycle_change(&package, &without_manifest, &without_manifest).is_none());
}

#[test]
fn packages_for_path_prefers_the_longest_matching_package_root() {
	let root = PathBuf::from("/repo");
	let packages = vec![
		PackageRecord::new(
			Ecosystem::Npm,
			"workspace",
			root.join("packages/package.json"),
			root.clone(),
			None,
			monochange_core::PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			None,
			monochange_core::PublishState::Public,
		),
	];

	let matched = packages_for_path(
		&root,
		&packages,
		&BTreeMap::new(),
		&BTreeMap::new(),
		Path::new("packages/web/src/index.ts"),
	);

	assert_eq!(matched.len(), 1);
	assert_eq!(
		matched
			.first()
			.unwrap_or_else(|| panic!("expected one matched package"))
			.name,
		"web"
	);
	assert!(
		packages_for_path(
			&root,
			&packages,
			&BTreeMap::new(),
			&BTreeMap::new(),
			Path::new("README.md")
		)
		.is_empty()
	);
}

#[test]
fn snapshot_discovery_covers_index_working_tree_and_write_failures() {
	let repo = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let destination = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	materialize_index_for_test(repo.path(), destination.path());
	assert!(destination.path().join("crates/core/Cargo.toml").is_file());

	let working_error = materialize_discovery_files(
		repo.path(),
		&SnapshotTarget::WorkingTree,
		destination.path(),
	)
	.expect_err("working-tree discovery should use the live workspace")
	.render();
	assert!(working_error.contains("working-tree discovery should use the existing workspace"));

	let parent_collision = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(parent_collision.path().join("occupied"), "file")
		.unwrap_or_else(|error| panic!("write parent collision: {error}"));
	let directory_error = write_discovery_file(
		parent_collision.path(),
		Path::new("occupied/Cargo.toml"),
		b"[package]",
	)
	.expect_err("a file in place of a directory should fail")
	.render();
	assert!(directory_error.contains("failed to create snapshot directory"));

	let file_collision = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir(file_collision.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("create file collision: {error}"));
	let file_error =
		write_discovery_file(file_collision.path(), Path::new("Cargo.toml"), b"[package]")
			.expect_err("a directory in place of a file should fail")
			.render();
	assert!(file_error.contains("failed to write snapshot file"));
}

fn materialize_index_for_test(repo_root: &Path, destination: &Path) {
	materialize_discovery_files(repo_root, &SnapshotTarget::GitIndex, destination)
		.unwrap_or_else(|error| panic!("materialize staged discovery files: {error}"));
}

#[test]
fn rebase_discovered_workspace_reports_outside_paths_and_rewrites_warnings() {
	let snapshot = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let snapshot_root = normalize_path(snapshot.path());
	let workspace_matcher = PackagePathMatcher::new("workspace", Path::new(""), &[], &[]);
	let make_workspace = |package: PackageRecord| {
		AnalysisWorkspace {
			cargo_semver_checks: CargoSemverChecksSettings::default(),
			packages: vec![package],
			release_identities: BTreeMap::new(),
			path_matchers: BTreeMap::new(),
			workspace_path_matchers: BTreeMap::new(),
			workspace_path_matcher: workspace_matcher.clone(),
			warnings: Vec::new(),
		}
	};

	let outside_manifest = PackageRecord::new(
		Ecosystem::Cargo,
		"outside-manifest",
		PathBuf::from("/outside/Cargo.toml"),
		snapshot_root.clone(),
		None,
		monochange_core::PublishState::Public,
	);
	let manifest_error = rebase_discovered_workspace(
		&mut make_workspace(outside_manifest),
		snapshot.path(),
		repo.path(),
	)
	.expect_err("an outside manifest should fail rebasing")
	.render();
	assert!(manifest_error.contains("snapshot package manifest"));

	let outside_workspace = PackageRecord::new(
		Ecosystem::Cargo,
		"outside-workspace",
		snapshot_root.join("Cargo.toml"),
		PathBuf::from("/outside"),
		None,
		monochange_core::PublishState::Public,
	);
	let workspace_error = rebase_discovered_workspace(
		&mut make_workspace(outside_workspace),
		snapshot.path(),
		repo.path(),
	)
	.expect_err("an outside workspace should fail rebasing")
	.render();
	assert!(workspace_error.contains("snapshot package workspace"));

	let inside = PackageRecord::new(
		Ecosystem::Cargo,
		"inside",
		snapshot_root.join("crates/inside/Cargo.toml"),
		snapshot_root,
		None,
		monochange_core::PublishState::Public,
	);
	let mut workspace = make_workspace(inside);
	workspace.warnings.push(format!(
		"discovery warning at {}",
		snapshot.path().display()
	));
	rebase_discovered_workspace(&mut workspace, snapshot.path(), repo.path())
		.unwrap_or_else(|error| panic!("rebase valid workspace: {error}"));
	assert_eq!(
		workspace.packages[0].manifest_path,
		repo.path().join("crates/inside/Cargo.toml")
	);
	assert!(workspace.warnings[0].contains(&repo.path().display().to_string()));
}

#[test]
fn snapshot_helpers_cover_error_paths_and_filtered_content() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let root = tempdir.path().to_path_buf();
	let head = git_output_trimmed(&root, &["rev-parse", "HEAD"]);
	let large_file = root.join("crates/core/src/large.rs");
	fs::write(&large_file, "a".repeat(300_000))
		.unwrap_or_else(|error| panic!("write large file: {error}"));
	git(&root, &["add", "."]);
	git(&root, &["commit", "-m", "add large file"]);

	assert!(read_working_tree_text(&large_file).is_none());
	assert!(
		read_text_file_from_git_object(&root, "HEAD:missing.rs")
			.unwrap_or_else(|error| panic!("read missing git object: {error}"))
			.is_none()
	);
	assert!(
		read_text_file_from_git_object(&root, "HEAD:crates/core/src/large.rs")
			.unwrap_or_else(|error| panic!("read large git object: {error}"))
			.is_none()
	);
	assert!(should_skip_directory(Path::new("target")));
	assert!(!should_skip_directory(Path::new("src")));
	let working_label = snapshot_label(&root, &SnapshotTarget::WorkingTree)
		.unwrap_or_else(|error| panic!("working label: {error}"));
	let index_label = snapshot_label(&root, &SnapshotTarget::GitIndex)
		.unwrap_or_else(|error| panic!("index label: {error}"));
	assert_eq!(working_label.len(), 40);

	assert_eq!(index_label.len(), 40);
	assert_eq!(
		snapshot_label(&root, &SnapshotTarget::GitRevision(head.clone()))
			.unwrap_or_else(|error| panic!("revision label: {error}")),
		git_tree_object(&root, &head)
			.unwrap_or_else(|error| panic!("resolve expected tree: {error}"))
	);

	let outside_package = PackageRecord::new(
		Ecosystem::Cargo,
		"outside",
		PathBuf::from("/outside/Cargo.toml"),
		PathBuf::from("/outside"),
		None,
		monochange_core::PublishState::Public,
	);
	assert!(
		snapshot_package(
			&root,
			&outside_package,
			&SnapshotTarget::WorkingTree,
			"label"
		)
		.is_err()
	);

	let not_a_repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let git_list_error = git_list_files(not_a_repo.path(), &["ls-files"])
		.unwrap_err()
		.render();
	assert!(git_list_error.contains("git"));
	let missing_repo = root.join("missing-repo");
	assert!(read_text_file_from_git_object(&missing_repo, "HEAD:file.rs").is_err());
	let index = not_a_repo.path().join("index");
	let index_error = git_with_index(not_a_repo.path(), &index, &["write-tree"])
		.expect_err("writing a tree outside a repository should fail")
		.render();
	assert!(index_error.contains("failed while resolving a semantic-analysis snapshot"));
}

#[test]
fn snapshot_target_helpers_cover_branch_range_pr_and_index_paths() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let root = tempdir.path().to_path_buf();
	git(&root, &["branch", "feature"]);

	let staged_targets = resolve_snapshot_targets(&root, &ChangeFrame::StagedOnly)
		.unwrap_or_else(|error| panic!("resolve staged targets: {error}"));
	assert!(matches!(staged_targets.after, SnapshotTarget::GitIndex));

	let range_targets = resolve_snapshot_targets(
		&root,
		&ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		},
	)
	.unwrap_or_else(|error| panic!("resolve branch targets: {error}"));
	assert!(matches!(
		range_targets.before,
		SnapshotTarget::GitRevision(_)
	));
	assert!(matches!(
		range_targets.after,
		SnapshotTarget::GitRevision(_)
	));

	let pr_targets = resolve_snapshot_targets(
		&root,
		&ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		},
	)
	.unwrap_or_else(|error| panic!("resolve pr targets: {error}"));
	assert!(matches!(pr_targets.before, SnapshotTarget::GitRevision(_)));

	let package_root = Path::new("crates/core");
	fs::create_dir_all(root.join("crates/core/target/generated"))
		.unwrap_or_else(|error| panic!("create skipped directory: {error}"));
	fs::write(
		root.join("crates/core/target/generated/ignored.rs"),
		"pub struct Ignored;\n",
	)
	.unwrap_or_else(|error| panic!("write skipped file: {error}"));
	let working_files = snapshot_files_from_working_tree(&root, package_root)
		.unwrap_or_else(|error| panic!("working tree snapshot: {error}"));
	assert!(!working_files.is_empty());
	assert!(
		snapshot_files_from_working_tree(&root, Path::new("missing"))
			.unwrap()
			.is_empty()
	);

	fs::write(root.join("crates/core/src/lib.rs"), "pub struct Changed;\n")
		.unwrap_or_else(|error| panic!("rewrite lib.rs: {error}"));
	git(&root, &["add", "crates/core/src/lib.rs"]);

	let index_files = snapshot_files_from_index(&root, package_root)
		.unwrap_or_else(|error| panic!("index snapshot: {error}"));
	assert!(
		index_files
			.iter()
			.any(|file| file.path == Path::new("src/lib.rs"))
	);
	assert!(
		read_text_file_from_target(
			&root,
			&SnapshotTarget::GitIndex,
			Path::new("crates/core/src/lib.rs")
		)
		.unwrap_or_else(|error| panic!("read index target: {error}"))
		.is_some()
	);
	assert!(
		read_text_file_from_target_with_limit(
			&root,
			&SnapshotTarget::GitIndex,
			Path::new("crates/core/src/lib.rs"),
			MAX_DISCOVERY_FILE_SIZE,
		)
		.unwrap_or_else(|error| panic!("read index target with discovery limit: {error}"))
		.is_some()
	);
	assert!(
		read_text_file_from_target(
			&root,
			&SnapshotTarget::GitRevision(git_output_trimmed(&root, &["rev-parse", "HEAD"])),
			Path::new("crates/core/src/lib.rs"),
		)
		.unwrap_or_else(|error| panic!("read revision target: {error}"))
		.is_some()
	);
	assert!(
		read_text_file_from_target_with_limit(
			&root,
			&SnapshotTarget::GitRevision(git_output_trimmed(&root, &["rev-parse", "HEAD"])),
			Path::new("crates/core/src/lib.rs"),
			MAX_DISCOVERY_FILE_SIZE,
		)
		.unwrap_or_else(|error| panic!("read revision target with discovery limit: {error}"))
		.is_some()
	);

	let built = build_git_object_snapshot_files_with_limit(
		&root,
		package_root,
		":",
		&[
			PathBuf::from("outside.txt"),
			PathBuf::from("crates/core/src/lib.rs"),
			PathBuf::from("crates/core/src/missing.rs"),
		],
		MAX_SNAPSHOT_FILE_SIZE,
	)
	.unwrap_or_else(|error| panic!("build snapshot files: {error}"));
	assert_eq!(built.len(), 1);
	assert_eq!(
		built
			.first()
			.unwrap_or_else(|| panic!("expected one built snapshot file"))
			.path,
		PathBuf::from("src/lib.rs")
	);
}

#[test]
fn auto_release_trajectory_resolution_uses_latest_workspace_tag_and_branch_refs() {
	let release = fixture_path("analysis/release-trajectory/release");
	let main = fixture_path("analysis/release-trajectory/main");
	let head = fixture_path("analysis/release-trajectory/head");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&release, root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);
	git(root, &["tag", "pkg-a/v9.9.9"]);

	git(root, &["tag", "v1.0.0"]);

	copy_directory(&main, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main evolution"]);
	git(root, &["checkout", "-b", "feature"]);

	copy_directory(&head, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "feature changes"]);

	let latest_tag = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(latest_workspace_release_tag(root))
		.unwrap_or_else(|error| panic!("latest workspace tag: {error}"));
	assert_eq!(latest_tag, "v1.0.0");
	assert_eq!(
		default_branch_ref(root).unwrap_or_else(|error| panic!("default branch: {error}")),
		"main"
	);

	let analysis = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(analyze_release_trajectory(root, &AnalysisConfig::default()))
		.unwrap_or_else(|error| panic!("release trajectory: {error}"));
	assert_eq!(analysis.refs.release_ref, "v1.0.0");
	assert_eq!(analysis.refs.main_ref, "main");
	assert_eq!(analysis.refs.head_ref, "feature");
	assert!(analysis.warnings.is_empty());
	assert!(
		analysis
			.frames
			.main_to_head
			.package_analyses
			.get("core")
			.unwrap_or_else(|| panic!("missing core package analysis"))
			.semantic_changes
			.iter()
			.any(|change| change.item_path == "shout")
	);
}

#[test]
fn git_and_snapshot_helpers_cover_remaining_error_paths() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let root = tempdir.path().to_path_buf();
	let package_root = Path::new("crates/core");
	let large_file = root.join("crates/core/src/large.rs");
	fs::write(&large_file, "a".repeat(300_000))
		.unwrap_or_else(|error| panic!("write large file: {error}"));

	let medium_file = root.join("crates/core/src/medium.rs");
	fs::write(&medium_file, "m".repeat(2_000))
		.unwrap_or_else(|error| panic!("write medium file: {error}"));
	let exact_limit_file = root.join("crates/core/src/exact.rs");
	fs::write(&exact_limit_file, "e".repeat(256 * 1024))
		.unwrap_or_else(|error| panic!("write exact limit file: {error}"));
	let file_named_target = root.join("crates/core/src/target");
	fs::write(&file_named_target, "not a directory")
		.unwrap_or_else(|error| panic!("write target file: {error}"));
	git(&root, &["add", "."]);
	git(&root, &["commit", "-m", "add fixture files"]);

	let working_files = snapshot_files_from_working_tree(&root, package_root)
		.unwrap_or_else(|error| panic!("working tree snapshot: {error}"));
	assert!(
		!working_files
			.iter()
			.any(|file| file.path == Path::new("src/large.rs"))
	);
	assert!(
		working_files
			.iter()
			.any(|file| file.path == Path::new("src/medium.rs"))
	);
	assert!(
		working_files
			.iter()
			.any(|file| file.path == Path::new("src/target")),
		"a file named 'target' should be included in the snapshot"
	);
	assert_eq!(
		read_working_tree_text(&medium_file),
		Some("m".repeat(2_000))
	);
	assert_eq!(
		read_text_file_from_git_object(&root, "HEAD:crates/core/src/medium.rs")
			.unwrap_or_else(|error| panic!("read medium git object: {error}")),
		Some("m".repeat(2_000))
	);
	assert_eq!(
		read_text_file_from_git_object(&root, "HEAD:crates/core/src/exact.rs")
			.unwrap_or_else(|error| panic!("read exact-size git object: {error}")),
		Some("e".repeat(256 * 1024))
	);

	let merge_base_error = git_merge_base(&root, "main", "missing-branch")
		.unwrap_err()
		.to_string();
	assert!(merge_base_error.contains("git merge-base main missing-branch failed"));

	fs::write(root.join("crates/core/src/lib.rs"), "pub struct Indexed;\n")
		.unwrap_or_else(|error| panic!("rewrite lib.rs: {error}"));
	git(&root, &["add", "crates/core/src/lib.rs"]);

	let package = discover_analysis_workspace(&root)
		.unwrap_or_else(|error| panic!("discover analysis workspace: {error}"))
		.packages
		.into_iter()
		.find(|package| package.name == "core")
		.unwrap_or_else(|| panic!("missing core package"));
	let index_snapshot = snapshot_package(&root, &package, &SnapshotTarget::GitIndex, "label")
		.unwrap_or_else(|error| panic!("index package snapshot: {error}"));
	assert!(
		index_snapshot
			.files
			.iter()
			.any(|file| file.path == Path::new("src/lib.rs"))
	);

	let spawn_error = git_list_files(&root.join("missing-repo"), &["ls-files"])
		.unwrap_err()
		.render();
	assert!(spawn_error.contains("failed to run git [\"ls-files\"]"));

	let binary_file = root.join("crates/core/src/invalid.bin");
	fs::write(&binary_file, [0_u8, 159, 146, 150])
		.unwrap_or_else(|error| panic!("write invalid binary file: {error}"));
	git(&root, &["add", "crates/core/src/invalid.bin"]);
	git(&root, &["commit", "-m", "add invalid binary file"]);

	let utf8_error = git_list_files(&root, &["show", "HEAD:crates/core/src/invalid.bin"])
		.unwrap_err()
		.render();
	assert!(utf8_error.contains("invalid utf-8"));
}
