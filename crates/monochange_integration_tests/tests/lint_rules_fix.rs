//! Integration tests that run every lint rule through `monochange check` and
//! `monochange check --fix` against file fixtures, verifying that autofixes
//! repair manifests in the expected way without clobbering unrelated content.
//!
//! Each ecosystem fixture under `fixtures/tests/lint-rules/` violates every
//! rule of its suite (fixable and diagnostic-only). The tests snapshot the
//! reported diagnostics, the fixed manifest contents, and the converged
//! fixpoint, and assert the fixed files still parse in their target format.

use std::ffi::OsString;
use std::path::Path;

use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_fixture(base: &str, name: &str) -> TempDir {
	let source =
		Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/tests/{base}/{name}"));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

fn run_check(root: &Path, args: &[&str]) -> String {
	// The CLI resolves the working directory to its canonical form (via
	// `std::env::current_dir`), while macOS tempdirs live behind the `/tmp`
	// symlink. Canonicalize so path comparisons inside the CLI (such as
	// `npm/root-no-prod-deps`'s root-manifest check) see consistent paths.
	let canonical_root =
		std::fs::canonicalize(root).unwrap_or_else(|error| panic!("canonicalize: {error}"));
	let mut cli_args = vec![OsString::from("monochange"), OsString::from("check")];
	cli_args.extend(args.iter().map(OsString::from));
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));
	let result = runtime.block_on(monochange::run_with_args_in_dir(
		"monochange",
		cli_args,
		&canonical_root,
	));
	match result {
		Ok(output) => output,
		Err(error) => {
			error
				.reported_output()
				.map_or_else(|| error.to_string(), ToOwned::to_owned)
		}
	}
}

fn normalize_workspace_paths(root: &Path, output: String) -> String {
	let canonical =
		std::fs::canonicalize(root).unwrap_or_else(|error| panic!("canonicalize root: {error}"));
	let canonical_path = canonical.to_string_lossy();
	let root_path = root.to_string_lossy();
	output
		.replace(canonical_path.as_ref(), "[workspace]")
		.replace(root_path.as_ref(), "[workspace]")
}

/// Run `check --fix` until no auto-fixable issues remain (bounded), returning
/// the final output.
///
/// The CLI signals remaining fixes with "remain auto-fixable" (after applying)
/// or "can be auto-fixed" (when nothing was fixable); a converged run reports
/// "No auto-fixable issues found." instead.
fn has_remaining_fixes(output: &str) -> bool {
	output.contains("remain auto-fixable") || output.contains("can be auto-fixed")
}

fn run_fix_to_fixpoint(root: &Path, max_iterations: usize) -> String {
	let mut output = String::new();
	for _ in 0..max_iterations {
		output = run_check(root, &["--format", "text", "--fix"]);
		if !has_remaining_fixes(&output) {
			break;
		}
	}
	output
}

fn read_manifest(root: &Path, relative: &str) -> String {
	std::fs::read_to_string(root.join(relative))
		.unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

// ── Cargo ────────────────────────────────────────────────────────────────────

#[test]
fn cargo_lint_reports_every_rule_without_fix() {
	let fixture = setup_fixture("lint-rules", "cargo");
	let output = run_check(fixture.path(), &["--format", "text"]);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
}

#[test]
fn cargo_fix_single_pass_applies_one_rewrite_per_file() {
	let fixture = setup_fixture("lint-rules", "cargo");
	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));

	// A single pass applies at most one whole-file rewrite per manifest; the
	// CLI asks for another run. Snapshot the partially fixed core manifest so
	// this behavior stays visible and intentional.
	let core = read_manifest(fixture.path(), "crates/core/Cargo.toml");
	insta::assert_snapshot!("cargo_single_pass_core_manifest", core);
}

#[test]
fn cargo_fix_converges_without_losing_content() {
	let fixture = setup_fixture("lint-rules", "cargo");
	let output = run_fix_to_fixpoint(fixture.path(), 8);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output.clone()));

	// Every manifest must still parse as TOML at the fixpoint.
	for manifest in [
		"crates/core/Cargo.toml",
		"crates/utils/Cargo.toml",
		"crates/orphan/Cargo.toml",
		"crates/private-dep/Cargo.toml",
	] {
		let contents = read_manifest(fixture.path(), manifest);
		contents
			.parse::<toml_edit::DocumentMut>()
			.unwrap_or_else(|error| panic!("{manifest} must stay valid TOML: {error}\n{contents}"));
	}

	// Unrelated content must survive every whole-file rewrite.
	let core = read_manifest(fixture.path(), "crates/core/Cargo.toml");
	assert!(
		core.contains("name = \"core\""),
		"lost package name:\n{core}"
	);
	assert!(
		core.contains("version = \"0.1.0\""),
		"lost version:\n{core}"
	);
	assert!(core.contains("edition = \"2021\""), "lost edition:\n{core}");
	assert!(
		core.contains("description = \"Core lint fixture package\""),
		"lost description:\n{core}"
	);
	assert!(
		core.contains("serde = \"1.0\""),
		"lost unrelated dependency:\n{core}"
	);
	assert!(
		core.contains("[dev-dependencies]"),
		"lost dev-dependencies section:\n{core}"
	);
	assert!(
		core.contains("repository = \"https://github.com/acme/lint-rules/tree/main/crates/core\""),
		"repository not fixed:\n{core}"
	);
	assert!(
		core.contains("utils = { workspace = true }")
			&& core.contains("private-dep = { workspace = true }"),
		"internal dependencies not rewritten:\n{core}"
	);

	let utils = read_manifest(fixture.path(), "crates/utils/Cargo.toml");
	assert!(
		utils.contains("description = \"Utils lint fixture package\""),
		"lost description:\n{utils}"
	);
	assert!(
		utils
			.contains("repository = \"https://github.com/acme/lint-rules/tree/main/crates/utils\""),
		"workspace-inherited repository not resolved:\n{utils}"
	);

	let orphan = read_manifest(fixture.path(), "crates/orphan/Cargo.toml");
	assert!(
		orphan.contains("publish = false"),
		"orphan not marked private:\n{orphan}"
	);

	// The only remaining diagnostics are the two intentionally non-fixable
	// rules; nothing fixable may survive the loop.
	assert!(
		!has_remaining_fixes(&output),
		"fix loop did not converge:\n{output}"
	);
	assert!(
		output.contains("cargo/publishable-dependencies"),
		"non-fixable publishable-dependencies diagnostic vanished:\n{output}"
	);
	assert!(
		output.contains("cargo/required-package-fields"),
		"non-fixable required-package-fields diagnostic vanished:\n{output}"
	);

	insta::assert_snapshot!("cargo_fixpoint_core_manifest", core);
	insta::assert_snapshot!("cargo_fixpoint_utils_manifest", utils);
	insta::assert_snapshot!("cargo_fixpoint_orphan_manifest", orphan);
}

// ── npm ──────────────────────────────────────────────────────────────────────

#[test]
fn npm_lint_reports_every_rule_without_fix() {
	let fixture = setup_fixture("lint-rules", "npm");
	let output = run_check(fixture.path(), &["--format", "text"]);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
}

#[test]
fn npm_fix_converges_without_losing_content() {
	let fixture = setup_fixture("lint-rules", "npm");
	let output = run_fix_to_fixpoint(fixture.path(), 8);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output.clone()));

	// Every manifest must still parse as JSON at the fixpoint.
	let root = read_manifest(fixture.path(), "package.json");
	let app = read_manifest(fixture.path(), "packages/app/package.json");
	let shared = read_manifest(fixture.path(), "packages/shared/package.json");
	let orphan = read_manifest(fixture.path(), "packages/orphan/package.json");
	for (name, contents) in [
		("package.json", &root),
		("packages/app/package.json", &app),
		("packages/shared/package.json", &shared),
		("packages/orphan/package.json", &orphan),
	] {
		let parsed: serde_json::Value = serde_json::from_str(contents)
			.unwrap_or_else(|error| panic!("{name} must stay valid JSON: {error}\n{contents}"));
		assert!(
			parsed.get("name").is_some(),
			"{name} lost its name field:\n{contents}"
		);
	}

	// Root: production dependencies moved to devDependencies, everything else kept.
	assert!(
		root.contains("\"devDependencies\""),
		"dependencies not moved to devDependencies:\n{root}"
	);
	assert!(
		!root.contains("\"dependencies\""),
		"production dependencies still present:\n{root}"
	);
	assert!(
		root.contains("\"left-pad\": \"1.3.0\""),
		"lost the moved dependency:\n{root}"
	);
	assert!(
		root.contains("\"description\": \"Root workspace fixture\""),
		"lost description:\n{root}"
	);
	assert!(
		root.contains("\"repository\": \"https://github.com/acme/lint-rules\""),
		"lost repository:\n{root}"
	);

	// App: workspace protocol applied, duplicate removed from dependencies,
	// repository fixed, unrelated fields intact.
	assert!(
		app.contains("\"shared\": \"workspace:*\""),
		"workspace protocol not applied:\n{app}"
	);
	assert!(
		app.contains("\"description\": \"App fixture package\""),
		"lost description:\n{app}"
	);
	assert!(
		app.contains("\"zod\": \"3.0.0\""),
		"lost unrelated dependency:\n{app}"
	);
	assert!(
		app.contains(
			"\"repository\": \"https://github.com/acme/lint-rules/tree/main/packages/app\""
		),
		"repository not fixed:\n{app}"
	);

	// Shared: missing repository inserted.
	assert!(
		shared.contains(
			"\"repository\": \"https://github.com/acme/lint-rules/tree/main/packages/shared\""
		),
		"repository not inserted:\n{shared}"
	);

	// Orphan: marked private.
	assert!(
		orphan.contains("\"private\": true"),
		"orphan not private:\n{orphan}"
	);

	// Only the non-fixable required-package-fields diagnostics may remain.
	assert!(
		!has_remaining_fixes(&output),
		"fix loop did not converge:\n{output}"
	);
	assert!(
		output.contains("npm/required-package-fields"),
		"non-fixable required-package-fields diagnostic vanished:\n{output}"
	);

	insta::assert_snapshot!("npm_fixpoint_root_manifest", root);
	insta::assert_snapshot!("npm_fixpoint_app_manifest", app);
	insta::assert_snapshot!("npm_fixpoint_shared_manifest", shared);
	insta::assert_snapshot!("npm_fixpoint_orphan_manifest", orphan);
}

// ── Dart ─────────────────────────────────────────────────────────────────────

#[test]
fn dart_lint_reports_every_rule_without_fix() {
	let fixture = setup_fixture("lint-rules", "dart");
	let output = run_check(fixture.path(), &["--format", "text"]);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
}

#[test]
fn dart_fix_converges_without_losing_content() {
	let fixture = setup_fixture("lint-rules", "dart");
	let output = run_fix_to_fixpoint(fixture.path(), 8);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output.clone()));

	// Every manifest must still parse as YAML at the fixpoint.
	let app = read_manifest(fixture.path(), "packages/app/pubspec.yaml");
	let helper = read_manifest(fixture.path(), "packages/helper/pubspec.yaml");
	let orphan = read_manifest(fixture.path(), "packages/orphan/pubspec.yaml");
	for (name, contents) in [
		("packages/app/pubspec.yaml", &app),
		("packages/helper/pubspec.yaml", &helper),
		("packages/orphan/pubspec.yaml", &orphan),
	] {
		let parsed: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(contents)
			.unwrap_or_else(|error| panic!("{name} must stay valid YAML: {error}\n{contents}"));
		assert!(
			!parsed.is_empty(),
			"{name} unexpectedly empty after fixes:\n{contents}"
		);
	}

	// App: assets and fonts sorted, dependencies sorted, repository inserted,
	// and every intentionally violated diagnostic still present with its
	// original content untouched.
	assert!(
		app.contains("name: app") && app.contains("version: 0.1.0"),
		"lost package identity:\n{app}"
	);
	assert!(
		app.contains("description: App fixture package"),
		"lost description:\n{app}"
	);
	assert!(
		app.contains("- assets/a.txt") && app.contains("- assets/b.txt"),
		"lost assets:\n{app}"
	);
	assert!(
		app.contains("- family: Alpha") && app.contains("- family: Zed"),
		"fonts not sorted:\n{app}"
	);
	assert!(
		app.contains("- asset: fonts/alpha-a.ttf") && app.contains("- asset: fonts/alpha-b.ttf"),
		"font assets not sorted:\n{app}"
	);
	assert!(
		app.contains("url: https://github.com/example/gitdep.git"),
		"lost git dependency:\n{app}"
	);
	assert!(
		app.contains("dependency_overrides:") && app.contains("path: ../vendored-http"),
		"lost dependency_overrides:\n{app}"
	);
	assert!(
		app.contains("sdk: '>=2.12.0 <4.0.0'") || app.contains("sdk: \">=2.12.0 <4.0.0\""),
		"lost sdk constraint:\n{app}"
	);
	assert!(
		app.contains("repository: https://github.com/acme/lint-rules/tree/main/packages/app"),
		"repository not inserted:\n{app}"
	);

	// Helper: repository inserted.
	assert!(
		helper.contains("repository: https://github.com/acme/lint-rules/tree/main/packages/helper"),
		"helper repository not inserted:\n{helper}"
	);

	// Orphan: marked private.
	assert!(
		orphan.contains("publish_to: none"),
		"orphan not private:\n{orphan}"
	);

	// Only the intentionally non-fixable diagnostics may remain.
	assert!(
		!has_remaining_fixes(&output),
		"fix loop did not converge:\n{output}"
	);
	for rule in [
		"dart/flutter-package-metadata-consistent",
		"dart/internal-path-dependency-policy",
		"dart/no-git-dependencies-in-published-packages",
		"dart/no-unexpected-dependency-overrides",
		"dart/required-package-fields",
		"dart/sdk-constraint-modern",
		"dart/sdk-constraint-present",
		"dart/workspace-internal-version-consistency",
	] {
		assert!(
			output.contains(rule),
			"non-fixable {rule} diagnostic vanished:\n{output}"
		);
	}

	insta::assert_snapshot!("dart_fixpoint_app_manifest", app);
	insta::assert_snapshot!("dart_fixpoint_helper_manifest", helper);
	insta::assert_snapshot!("dart_fixpoint_orphan_manifest", orphan);
}

// ── Changesets ───────────────────────────────────────────────────────────────

#[test]
fn changesets_lint_reports_every_rule_without_fix() {
	let fixture = setup_fixture("lint-rules", "changesets");
	let output = run_check(fixture.path(), &["--format", "text"]);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
}

#[test]
fn changesets_fix_converts_object_form_to_inline_without_touching_bodies() {
	let fixture = setup_fixture("lint-rules", "changesets");
	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output.clone()));

	// The prefer-inline fix converts frontmatter object entries to the inline
	// form with a targeted span edit; the summary bodies must be untouched.
	let object_form = read_manifest(fixture.path(), ".changeset/object-form.md");
	assert!(
		object_form.contains("app: feat"),
		"frontmatter not converted to inline form:\n{object_form}"
	);
	assert!(
		object_form.contains("## Add object form conversion")
			&& object_form.contains("which prefer-inline converts to inline"),
		"changeset body was modified:\n{object_form}"
	);

	let section_headings = read_manifest(fixture.path(), ".changeset/section-headings.md");
	assert!(
		section_headings.contains("app: fix"),
		"frontmatter not converted to inline form:\n{section_headings}"
	);
	assert!(
		section_headings.contains("## Fix"),
		"section heading should remain (non-fixable diagnostic):\n{section_headings}"
	);

	// The non-fixable diagnostics must survive and the clean changeset must
	// remain byte-identical.
	assert!(
		output.contains("changesets/summary") && output.contains("changesets/no_section_headings"),
		"non-fixable diagnostics vanished:\n{output}"
	);
	let clean = read_manifest(fixture.path(), ".changeset/clean.md");
	insta::assert_snapshot!("changesets_fixpoint_clean_changeset", clean);
	insta::assert_snapshot!("changesets_fixpoint_object_form", object_form);
	insta::assert_snapshot!("changesets_fixpoint_section_headings", section_headings);
}
