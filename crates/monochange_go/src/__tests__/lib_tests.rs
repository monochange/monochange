use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;

use crate::discover_go_modules;
use crate::update_go_mod_text;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn discover_go_modules_stores_relative_path_metadata() {
	let root = fixture_path("go/multi-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let api = discovery.packages.iter().find(|p| p.name == "api").unwrap();
	assert_eq!(
		api.metadata.get("relative_path").map(String::as_str),
		Some("api")
	);
}

// -- default_lockfile_commands --
#[test]
fn default_lockfile_commands_infers_go_mod_tidy() {
	let root = fixture_path("go/single-module");
	let package = PackageRecord::new(
		Ecosystem::Go,
		"myapp",
		root.join("go.mod"),
		root.clone(),
		None,
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert_eq!(commands.first().unwrap().command, "go mod tidy");
}

#[test]
fn update_go_mod_text_major_version_suffix_require_matches_full_path_key() {
	let input = "module app\n\nrequire github.com/example/shared/v2 v2.0.0\n";
	let deps = BTreeMap::from([(
		"github.com/example/shared/v2".to_string(),
		"v2.1.0".to_string(),
	)]);
	let result = update_go_mod_text(input, &deps);
	assert!(
		result.contains("require github.com/example/shared/v2 v2.1.0"),
		"require with a major version suffix should still rewrite: {result}"
	);
}
