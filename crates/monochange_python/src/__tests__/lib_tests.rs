use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use tempfile::tempdir;

use crate::PythonAdapter;
use crate::PythonVersionedFileKind;
use crate::discover_python_packages;
use crate::update_versioned_file_text;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn discover_python_packages_preserves_native_names_and_supplies_canonical_dependency_aliases() {
	let root = fixture_path("python/poetry-producer-name-normalization/workspace");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover normalized producer: {error}"));
	let producer = discovery
		.packages
		.iter()
		.find(|package| package.name == "PY_Core")
		.unwrap_or_else(|| panic!("missing native producer name"));

	assert_eq!(
		producer
			.metadata
			.get(monochange_core::PACKAGE_DEPENDENCY_NAME_METADATA_KEY)
			.map(String::as_str),
		Some("py-core")
	);
	assert!(
		discovery
			.packages
			.iter()
			.filter(|package| package.name != "PY_Core")
			.all(|package| {
				!package
					.metadata
					.contains_key(monochange_core::PACKAGE_DEPENDENCY_NAME_METADATA_KEY)
			})
	);
}

#[test]
fn update_versioned_file_text_updates_poetry_dependency_constraints_and_preserves_metadata() {
	let input = fs::read_to_string(fixture_path(
		"python/poetry-dependency-update/pyproject.toml",
	))
	.unwrap_or_else(|error| panic!("read fixture: {error}"));
	let deps = [
		"py-core",
		"inline-core",
		"table-core",
		"path-core",
		"git-core",
		"invalid-core",
	]
	.into_iter()
	.map(|name| (name.to_string(), ">=2.0.0".to_string()))
	.collect();
	let result = update_versioned_file_text(&input, PythonVersionedFileKind::Manifest, None, &deps)
		.unwrap_or_else(|error| panic!("update Poetry dependencies: {error}"));
	let parsed: toml::Value =
		toml::from_str(&result).unwrap_or_else(|error| panic!("parse updated manifest: {error}"));

	assert_eq!(
		parsed["tool"]["poetry"]["dependencies"]["PY_Core"].as_str(),
		Some(">=2.0.0")
	);
	assert_eq!(
		parsed["tool"]["poetry"]["dependencies"]["inline-core"]["version"].as_str(),
		Some(">=2.0.0")
	);
	assert_eq!(
		parsed["tool"]["poetry"]["dependencies"]["table-core"]["version"].as_str(),
		Some(">=2.0.0")
	);
	assert_eq!(
		parsed["tool"]["poetry"]["group"]["dev"]["dependencies"]["py-core"]["version"].as_str(),
		Some(">=2.0.0")
	);
	insta::assert_snapshot!(result);
}

// -- load_configured_python_package --
#[test]
fn load_configured_python_package_loads_configured_uv_workspace_member() {
	use monochange_core::EcosystemAdapter;
	let root = fixture_path("python/uv-workspace");
	let package = PythonAdapter
		.load_configured(&root, &root.join("packages/core"))
		.unwrap_or_else(|error| panic!("load configured: {error}"))
		.unwrap_or_else(|| panic!("expected the configured package to be loaded"));
	assert_eq!(package.name, "my-core");
	assert_eq!(package.ecosystem, Ecosystem::Python);
	assert_eq!(package.current_version, Some(Version::new(1, 0, 0)));
	assert!(
		package
			.manifest_path
			.ends_with("packages/core/pyproject.toml")
	);
	assert!(
		!package.declared_dependencies.is_empty(),
		"configured package should keep discovered dependencies"
	);
}

// -- workspace member expansion with direct pyproject.toml files --
#[test]
fn expand_workspace_members_handles_glob_matching_pyproject_files() {
	use std::fs;
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Create workspace root with glob pattern that matches pyproject.toml directly
	let pkg_dir = root.join("packages/core");
	fs::create_dir_all(&pkg_dir).unwrap();
	fs::write(
		pkg_dir.join("pyproject.toml"),
		"[project]\nname = \"core\"\nversion = \"1.0.0\"\ndependencies = []\n",
	)
	.unwrap();
	fs::write(
		root.join("pyproject.toml"),
		"[project]\nname = \"root\"\nversion = \"0.1.0\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
	)
	.unwrap();

	let discovery =
		discover_python_packages(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(discovery.packages.first().unwrap().name, "core");
}

#[test]
fn default_lockfile_commands_handles_uv_poetry_and_unknown_lockfiles() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("pyproject.toml"),
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write pyproject: {error}"));
	fs::write(root.join("uv.lock"), "").unwrap_or_else(|error| panic!("write uv.lock: {error}"));
	fs::write(root.join("poetry.lock"), "")
		.unwrap_or_else(|error| panic!("write poetry.lock: {error}"));
	fs::write(root.join("requirements.lock"), "")
		.unwrap_or_else(|error| panic!("write requirements.lock: {error}"));
	let package = PackageRecord::new(
		Ecosystem::Python,
		"app",
		root.join("pyproject.toml"),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Unpublished,
	);

	let commands = crate::default_lockfile_commands(&package);
	let command_names = commands
		.iter()
		.map(|command| command.command.as_str())
		.collect::<Vec<_>>();

	assert_eq!(command_names, vec!["uv lock", "poetry lock"]);
	assert_eq!(crate::lockfile_command("requirements.lock"), None);
	let root_name = root.file_name().unwrap_or_else(|| panic!("temp root name"));
	assert!(
		commands
			.iter()
			.all(|command| command.cwd.ends_with(root_name))
	);
}

#[test]
fn update_versioned_file_text_returns_early_without_project_dependencies() {
	let deps = BTreeMap::from([("core".to_string(), ">=2.0.0".to_string())]);

	let without_project = update_versioned_file_text(
		"[tool.custom]\nname = \"app\"\n",
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&deps,
	)
	.unwrap_or_else(|error| panic!("update without project: {error}"));
	assert!(!without_project.contains("2.0.0"));

	let without_dependencies = update_versioned_file_text(
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\n",
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&deps,
	)
	.unwrap_or_else(|error| panic!("update without dependencies: {error}"));
	assert!(without_dependencies.contains("version = \"2.0.0\""));
	assert!(!without_dependencies.contains("core>=2.0.0"));
}

#[test]
fn private_workspace_helpers_cover_error_and_match_branches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let manifest_path = root.join("pyproject.toml");
	let packages = root.join("packages");
	let package_dir = packages.join("core");
	fs::create_dir_all(&package_dir).unwrap_or_else(|error| panic!("create package dir: {error}"));
	fs::write(
		&manifest_path,
		"[project]\nname = \"root\"\nversion = \"1.0.0\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\", \"packages/core/pyproject.toml\", \"README.md\", \"missing/*\"]\n",
	)
	.unwrap_or_else(|error| panic!("write root pyproject: {error}"));
	fs::write(
		package_dir.join("pyproject.toml"),
		"[project]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write core pyproject: {error}"));

	fs::write(root.join("README.md"), "# docs\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));

	let members = crate::parse_uv_workspace_members(&manifest_path)
		.unwrap_or_else(|error| panic!("parse uv members: {error}"))
		.unwrap_or_else(|| panic!("expected uv workspace members"));
	assert!(members.contains(&"packages/*".to_string()));

	let missing_error = crate::parse_uv_workspace_members(&root.join("missing-pyproject.toml"))
		.err()
		.unwrap_or_else(|| panic!("expected missing pyproject error"));
	assert!(missing_error.to_string().contains("failed to read"));

	fs::write(root.join("invalid.toml"), "[project\n")
		.unwrap_or_else(|error| panic!("write invalid toml: {error}"));
	let parse_error = crate::parse_uv_workspace_members(&root.join("invalid.toml"))
		.err()
		.unwrap_or_else(|| panic!("expected invalid toml error"));
	assert!(parse_error.to_string().contains("failed to parse"));

	let mut warnings = Vec::new();
	let manifests = crate::expand_workspace_members(root, &members, &mut warnings);
	assert!(
		manifests
			.iter()
			.any(|manifest| manifest.ends_with("packages/core/pyproject.toml")),
		"missing core manifest in {manifests:?}"
	);
	assert!(warnings.iter().any(|warning| warning.contains("missing/*")));
}

#[test]
fn private_package_parser_covers_error_and_dependency_value_branches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let missing_error = crate::parse_python_package(&root.join("missing.toml"), root)
		.err()
		.unwrap_or_else(|| panic!("expected missing package error"));
	assert!(missing_error.to_string().contains("failed to read"));

	let invalid_path = root.join("invalid.toml");
	fs::write(&invalid_path, "[project\n").unwrap_or_else(|error| panic!("write invalid: {error}"));
	let parse_error = crate::parse_python_package(&invalid_path, root)
		.err()
		.unwrap_or_else(|| panic!("expected parse package error"));
	assert!(parse_error.to_string().contains("failed to parse"));

	let project = toml::from_str::<toml::Value>(
		r#"
[project]
dependencies = ["requests>=2", 1]

[project.optional-dependencies]
dev = ["pytest>=8", 1]
empty = "not-an-array"
"#,
	)
	.unwrap_or_else(|error| panic!("parse project fixture: {error}"));
	let project_table = project
		.get("project")
		.unwrap_or_else(|| panic!("expected project table"));
	let pep621_deps = crate::parse_pep621_dependencies(project_table);
	assert!(pep621_deps.iter().any(|dep| {
		dep.name == "requests"
			&& !dep.optional
			&& dep.source_field.as_deref() == Some("dependencies")
	}));
	assert!(pep621_deps.iter().any(|dep| {
		dep.name == "pytest"
			&& dep.optional
			&& dep.source_field.as_deref() == Some("optional-dependencies")
	}));

	let poetry = toml::from_str::<toml::Value>(
		r#"
[dependencies]
python = ">=3.11"
django = "^4.2"
celery = { version = "^5.3" }
local = { path = "../local" }
unknown = 1

[group]
metadata = "not-a-group-table"

[group.empty]
dependencies = "not-a-dependency-table"

[group.dev.dependencies]
pytest = "^8.0"
ruff = { version = "^0.4" }
tooling = { path = "../tooling" }
numbered = 1
"#,
	)
	.unwrap_or_else(|error| panic!("parse poetry fixture: {error}"));
	let empty_poetry = toml::Value::Table(toml::map::Map::new());
	assert!(crate::parse_poetry_dependencies(&empty_poetry).is_empty());

	let deps = crate::parse_poetry_dependencies(&poetry);
	assert!(deps.iter().any(|dep| {
		dep.name == "django"
			&& dep.version_constraint.as_deref() == Some("^4.2")
			&& dep.source_field.as_deref() == Some("dependencies")
	}));
	assert!(
		deps.iter()
			.any(|dep| dep.name == "celery" && dep.version_constraint.as_deref() == Some("^5.3"))
	);
	assert!(
		deps.iter()
			.any(|dep| dep.name == "local" && dep.version_constraint.is_none())
	);
	assert!(deps.iter().any(|dep| {
		dep.name == "ruff"
			&& dep.version_constraint.as_deref() == Some("^0.4")
			&& dep.source_field.as_deref() == Some("group.dependencies")
	}));
	assert!(
		deps.iter()
			.any(|dep| dep.name == "tooling" && dep.version_constraint.is_none())
	);
	assert!(!deps.iter().any(|dep| dep.name == "python"));
}
