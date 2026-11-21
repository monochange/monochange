use std::fs;

use monochange_config::load_workspace_configuration;

use super::*;

fn config() -> LintRuleConfig {
	LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::new(),
	}
}

// ---------------------------------------------------------------------------
// Autofix safety: no lint rule may ever replace a manifest with a fragment.
//
// Every fix these rules emit is either a targeted span edit or a whole-file
// rewrite produced by serializing a mutated copy of the parsed document
// (`LintFix::document`). These tests apply each rule's fixes through the real
// `monochange_lint` pipeline (`Linter::apply_fixes`) and assert the resulting
// manifest still parses and keeps all unrelated content.
//
// All manifests are written into temporary directories; tests must never rely
// on relative manifest paths, which would resolve against the crate's own
// sources.
// ---------------------------------------------------------------------------
use std::path::PathBuf;

use monochange_core::lint::LintReport;
use monochange_core::lint::WorkspaceLintSettings;
use monochange_lint::Linter;

/// The outcome of running a rule and applying its fixes through the pipeline.
struct FixApplication {
	/// Keep the temporary workspace alive for the assertions.
	_workspace: tempfile::TempDir,
	/// Results reported by the rule.
	results: Vec<LintResult>,
	/// Fixed contents, absent when the pipeline applied nothing.
	fixed: Option<String>,
}

/// Run `rule` over a manifest written into a temporary workspace, apply the
/// rule's fixes through the real fix pipeline, and return the outcome.
///
/// `relative_manifest_dir` places the manifest inside the workspace (empty
/// string for the workspace root). When `root_manifest` is given it is written
/// to the workspace root first, so workspace-inheritance scenarios can resolve
/// inherited values.
fn run_rule_and_apply_fixes(
	rule: &dyn LintRuleRunner,
	contents: &str,
	config: &LintRuleConfig,
	relative_manifest_dir: &str,
	root_manifest: Option<&str>,
	managed: bool,
) -> FixApplication {
	let workspace = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	if let Some(root_manifest) = root_manifest {
		fs::write(workspace.path().join("Cargo.toml"), root_manifest)
			.unwrap_or_else(|error| panic!("write root manifest: {error}"));
	}

	let manifest_dir = if relative_manifest_dir.is_empty() {
		workspace.path().to_path_buf()
	} else {
		let dir = workspace.path().join(relative_manifest_dir);
		fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create {dir:?}: {error}"));
		dir
	};

	let manifest_path = manifest_dir.join("Cargo.toml");
	fs::write(&manifest_path, contents).unwrap_or_else(|error| panic!("write manifest: {error}"));

	let relative_manifest = if relative_manifest_dir.is_empty() {
		PathBuf::from("Cargo.toml")
	} else {
		Path::new(relative_manifest_dir).join("Cargo.toml")
	};

	let target = LintTarget::new(
		workspace.path().to_path_buf(),
		manifest_path.clone(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: relative_manifest,
			package_name: Some("example".to_string()),
			package_id: Some("example".to_string()),
			group_id: None,
			managed,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(CargoLintFile {
			document: contents
				.parse::<DocumentMut>()
				.unwrap_or_else(|error| panic!("parse manifest: {error}")),
			workspace_package_names: Arc::new(BTreeSet::from(["internal_dep".to_string()])),
			workspace_package_publishable: Arc::new(BTreeMap::new()),
			repo_url: Arc::new("https://github.com/foo/bar".to_string()),
			default_branch: Arc::new(String::from("main")),
		}),
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = rule.run(&ctx, config);

	let linter = Linter::new(
		vec![Box::new(CargoLintSuite)],
		WorkspaceLintSettings::default(),
	);
	let mut report = LintReport::new();

	for result in results.iter().cloned() {
		report.add(result);
	}

	let fixed = linter.apply_fixes(&report).get(&manifest_path).cloned();

	FixApplication {
		_workspace: workspace,
		results,
		fixed,
	}
}

/// Assert `fixed` parses as TOML and keeps every top-level table from
/// `original`, so a fix can never replace a manifest with a fragment.
fn assert_fix_preserves_document(original: &str, fixed: &str) {
	let original_doc = original
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("original manifest must parse: {error}"));
	let fixed_doc = fixed
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("fixed manifest must parse as TOML: {error}\n{fixed}"));
	let original_root = original_doc.as_table();
	let fixed_root = fixed_doc.as_table();

	for key in original_root.iter().map(|(key, _)| key) {
		assert!(
			fixed_root.get(key).is_some(),
			"top-level table `{key}` was dropped by the fix:\n--- original ---\n{original}\n--- fixed ---\n{fixed}"
		);
	}
}

#[test]
fn manifest_repository_wrong_value_fix_preserves_manifest() {
	let rule = ManifestRepositoryRule::new();
	let contents = "[package]\nname = \"example\"\nversion = \"0.1.0\"\nrepository = \"https://wrong.example.com\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1.0\"\n";
	let application = run_rule_and_apply_fixes(&rule, contents, &config(), "", None, true);
	assert_eq!(application.results.len(), 1);
	let fixed = application
		.fixed
		.unwrap_or_else(|| panic!("expected a fix to apply"));

	// The whole manifest survives; only the repository value changes.
	assert!(
		fixed.contains("name = \"example\""),
		"lost package name: {fixed}"
	);
	assert!(
		fixed.contains("version = \"0.1.0\""),
		"lost version: {fixed}"
	);
	assert!(
		fixed.contains("edition = \"2021\""),
		"lost edition: {fixed}"
	);
	assert!(
		fixed.contains("[dependencies]"),
		"lost dependencies section: {fixed}"
	);
	assert!(
		fixed.contains("serde = \"1.0\""),
		"lost serde dependency: {fixed}"
	);
	assert!(
		fixed.contains("repository = \"https://github.com/foo/bar\""),
		"repository not updated: {fixed}"
	);
	assert!(
		!fixed.contains("wrong.example.com"),
		"stale value kept: {fixed}"
	);
	assert_fix_preserves_document(contents, &fixed);
}

#[test]
fn fix_loop_converges_without_losing_manifest_content() {
	// End-to-end: run the entire cargo suite over a workspace whose manifests
	// violate several rules at once, then apply fixes iteratively (like
	// `monochange check --fix`) until no fixable results remain. Every
	// intermediate manifest must keep parsing and every original field must
	// survive.
	let workspace = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = workspace.path();

	fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
package_type = "cargo"
changelog = false

[source]
provider = "github"
owner = "acme"
repo = "widgets"

[lints.rules]
"cargo/dependency-field-order" = "error"
"cargo/internal-dependency-workspace" = "error"
"cargo/required-package-fields" = "off"
"cargo/sorted-dependencies" = "error"
"cargo/unlisted-package-private" = { level = "error", fix = true }
"cargo/manifest-repository" = "error"

[ecosystems.cargo]
enabled = true

[package.core]
path = "crates/core"
type = "cargo"
version = "0.1.0"

[package.utils]
path = "crates/utils"
type = "cargo"
version = "0.1.0"
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nrepository = \"https://github.com/acme/widgets\"\n",
	)
	.unwrap_or_else(|error| panic!("write root manifest: {error}"));

	let core_dir = root.join("crates/core");
	fs::create_dir_all(&core_dir).unwrap_or_else(|error| panic!("create core dir: {error}"));
	let core_manifest = "[package]\nname = \"core\"\nversion = \"0.1.0\"\nrepository = { workspace = true }\n\n[dependencies]\nutils = \"0.1.0\"\nserde = \"1.0\"\n\n[dev-dependencies.serde]\nfeatures = [\"derive\"]\nworkspace = true\n";
	fs::write(core_dir.join("Cargo.toml"), core_manifest)
		.unwrap_or_else(|error| panic!("write core manifest: {error}"));

	let utils_dir = root.join("crates/utils");
	fs::create_dir_all(&utils_dir).unwrap_or_else(|error| panic!("create utils dir: {error}"));
	let utils_manifest = "[package]\nname = \"utils\"\nversion = \"0.1.0\"\n\n[dependencies]\nzlib = \"1.0\"\nserde = \"1.0\"\n";
	fs::write(utils_dir.join("Cargo.toml"), utils_manifest)
		.unwrap_or_else(|error| panic!("write utils manifest: {error}"));

	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load configuration: {error}"));
	let linter = Linter::new(
		vec![Box::new(CargoLintSuite)],
		WorkspaceLintSettings::default(),
	);

	for _iteration in 0..10 {
		let report = linter.lint_workspace(
			root,
			&configuration,
			&monochange_core::lint::NoopLintProgressReporter,
		);

		if report.autofixable().is_empty() {
			break;
		}

		for (path, fixed) in linter.apply_fixes(&report) {
			// Every intermediate write must produce a parseable manifest.
			fixed.parse::<DocumentMut>().unwrap_or_else(|error| {
				panic!(
					"fix loop produced unparseable {}: {error}\n{fixed}",
					path.display()
				)
			});
			fs::write(&path, fixed)
				.unwrap_or_else(|error| panic!("write fixed manifest {}: {error}", path.display()));
		}
	}

	let report = linter.lint_workspace(
		root,
		&configuration,
		&monochange_core::lint::NoopLintProgressReporter,
	);
	assert_eq!(
		report.autofixable().len(),
		0,
		"fix loop did not converge; remaining: {:?}",
		report
			.autofixable()
			.iter()
			.map(|result| format!("{}: {}", result.rule_id, result.message))
			.collect::<Vec<_>>()
	);

	let fixed_core = fs::read_to_string(core_dir.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read core manifest: {error}"));
	assert!(
		fixed_core.contains("name = \"core\""),
		"core package name lost: {fixed_core}"
	);
	assert!(
		fixed_core.contains("serde = \"1.0\""),
		"core serde lost: {fixed_core}"
	);
	assert!(
		fixed_core.contains("utils = { workspace = true }"),
		"core internal dependency not rewritten: {fixed_core}"
	);
	assert!(
		fixed_core
			.contains("repository = \"https://github.com/acme/widgets/tree/main/crates/core\""),
		"core repository not fixed: {fixed_core}"
	);
	let core_dev_section = fixed_core
		.split("[dev-dependencies.serde]")
		.nth(1)
		.unwrap_or_else(|| panic!("missing core dev-dependencies.serde section: {fixed_core}"));
	let core_workspace_pos = core_dev_section
		.find("workspace = true")
		.unwrap_or_else(|| panic!("core workspace field lost: {fixed_core}"));
	let core_features_pos = core_dev_section
		.find("features")
		.unwrap_or_else(|| panic!("core features field lost: {fixed_core}"));

	assert!(
		core_workspace_pos < core_features_pos,
		"core fields not reordered: {fixed_core}"
	);
	assert_fix_preserves_document(core_manifest, &fixed_core);

	let fixed_utils = fs::read_to_string(utils_dir.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read utils manifest: {error}"));
	assert!(
		fixed_utils.contains("name = \"utils\""),
		"utils package name lost: {fixed_utils}"
	);
	assert!(
		fixed_utils.contains("zlib = \"1.0\""),
		"utils zlib lost: {fixed_utils}"
	);
	assert!(
		fixed_utils.contains("serde = \"1.0\""),
		"utils serde lost: {fixed_utils}"
	);
	assert!(
		fixed_utils
			.contains("repository = \"https://github.com/acme/widgets/tree/main/crates/utils\""),
		"utils repository not fixed: {fixed_utils}"
	);
	assert_fix_preserves_document(utils_manifest, &fixed_utils);
}
