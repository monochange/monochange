use monochange_core::AnalyzedFileChange;
use monochange_core::FileChangeKind;

use super::*;

#[test]
fn snapshot_public_symbols_uses_changed_files_and_collects_warnings() {
	let changed_files = vec![
		AnalyzedFileChange {
			path: PathBuf::from("crates/core/src/lib.rs"),
			package_path: PathBuf::from("src/lib.rs"),
			kind: FileChangeKind::Modified,
			before_contents: None,
			after_contents: Some("pub struct Greeter;".to_string()),
		},
		AnalyzedFileChange {
			path: PathBuf::from("crates/core/src/helper.txt"),
			package_path: PathBuf::from("src/helper.txt"),
			kind: FileChangeKind::Modified,
			before_contents: None,
			after_contents: Some("ignored".to_string()),
		},
		AnalyzedFileChange {
			path: PathBuf::from("crates/core/src/bad.rs"),
			package_path: PathBuf::from("src/bad.rs"),
			kind: FileChangeKind::Modified,
			before_contents: Some("pub fn broken(".to_string()),
			after_contents: None,
		},
		AnalyzedFileChange {
			path: PathBuf::from("crates/core/src/empty.rs"),
			package_path: PathBuf::from("src/empty.rs"),
			kind: FileChangeKind::Modified,
			before_contents: None,
			after_contents: None,
		},
	];

	let (symbols, warnings) =
		snapshot_public_symbols(None, &changed_files, DetectionLevel::Signature);

	assert!(symbols.contains_key(&("struct".to_string(), "Greeter".to_string())));
	assert_eq!(warnings.len(), 1);
	assert!(
		warnings
			.first()
			.unwrap_or_else(|| panic!("expected one parse warning"))
			.contains("failed to parse src/bad.rs")
	);
}

#[test]
fn collect_public_symbols_covers_all_supported_public_item_kinds() {
	let file = PackageSnapshotFile {
		path: PathBuf::from("src/api.rs"),
		contents: concat!(
			"pub const LIMIT: usize = 3;\n",
			"pub enum Mode { Fast }\n",
			"pub static NAME: &str = \"core\";\n",
			"pub struct Greeter;\n",
			"pub trait Renderer {}\n",
			"pub type Greeting = String;\n",
			"pub union Number { value: u32 }\n",
			"pub use crate::helpers::render;\n",
		)
		.to_string(),
	};

	let symbols = collect_public_symbols(&file)
		.unwrap_or_else(|error| panic!("symbol extraction should succeed: {error}"));

	for expected in [
		"LIMIT",
		"Mode",
		"NAME",
		"Greeter",
		"Renderer",
		"Greeting",
		"Number",
		"crate :: helpers :: render",
	] {
		assert!(
			symbols
				.iter()
				.any(|symbol| symbol.item_path.ends_with(expected))
		);
	}
}

#[rstest::rstest]
#[case::constant(
	"pub const LIMIT: usize = 3;",
	"/// old documentation\npub const LIMIT: usize = 3;"
)]
#[case::static_item(
	"pub static NAME: &str = \"core\";",
	"#[doc = \"old documentation\"] pub static NAME: &str = \"core\";"
)]
#[case::struct_field(
	"pub struct Item { pub value: u8 }",
	"/// old documentation\npub struct Item { /// old documentation\n pub value: u8 }"
)]
#[case::tuple_field(
	"pub struct Item(pub u8);",
	"pub struct Item(#[doc = \"old documentation\"] pub u8);"
)]
#[case::enum_variant(
	"pub enum Mode { Fast { value: u8 }, Slow(u8) }",
	"/// old documentation\npub enum Mode { /// old documentation\n Fast { /// old documentation\n value: u8 }, Slow(#[doc = \"old documentation\"] u8) }"
)]
#[case::trait_items(
	"pub trait Renderer { const LIMIT: u8 = 3; type Output; fn render(&self) {} }",
	"/// old documentation\npub trait Renderer { /// old documentation\n const LIMIT: u8 = 3; /// old documentation\n type Output; /// old documentation\n fn render(&self) {} }"
)]
#[case::module_items(
	"pub mod api { pub struct Item; }",
	"/// old documentation\npub mod api { //! old documentation\n /// old documentation\n pub struct Item; }"
)]
#[case::type_alias(
	"pub type Name = String;",
	"#[doc = include_str!(\"guide.md\")] pub type Name = String;"
)]
#[case::union_field(
	"pub union Number { pub value: u32 }",
	"pub union Number { #[doc = \"old documentation\"] pub value: u32 }"
)]
#[case::doc_options(
	"pub struct Item;",
	"#[doc(hidden)] #[doc(alias = \"old documentation\")] pub struct Item;"
)]
#[case::conditional_doc(
	"pub struct Item;",
	"#[cfg_attr(doc, doc = \"old documentation\")] pub struct Item;"
)]
#[case::mixed_conditional(
	"#[cfg_attr(feature = \"ffi\", repr(C), derive(Clone))] pub struct Item;",
	"#[cfg_attr(feature = \"ffi\", repr(C), doc = \"old documentation\", derive(Clone))] pub struct Item;"
)]
#[case::nested_conditional(
	"#[cfg_attr(feature = \"ffi\", repr(C))] pub struct Item;",
	"#[cfg_attr(feature = \"ffi\", cfg_attr(doc, doc = \"old documentation\"), repr(C))] pub struct Item;"
)]
fn public_api_docs_do_not_change_signatures(#[case] plain: &str, #[case] documented: &str) {
	assert!(diff_public_symbols_allow_empty(plain, documented).is_empty());
	assert!(diff_public_symbols_allow_empty(documented, plain).is_empty());
	let edited = documented.replace("old documentation", "new documentation");
	assert!(diff_public_symbols_allow_empty(documented, &edited).is_empty());
}

#[test]
fn slice_append_covers_vec_and_fixed_size_array_declarations() {
	let appends = [
		(
			"pub const A: [u8; 2] = [1, 2];\n",
			"pub const A: [u8; 3] = [1, 2, 3];\n",
		),
		(
			"pub const B: &[u8] = &[1, 2];\n",
			"pub const B: &[u8] = &[1, 2, 3];\n",
		),
		(
			"pub static C: Vec<u8> = vec![1, 2];\n",
			"pub static C: Vec<u8> = vec![1, 2, 3];\n",
		),
	];

	for (before_source, after_source) in appends {
		let change = diff_one_constant(before_source, after_source);
		let assessment = change
			.assessment
			.as_ref()
			.unwrap_or_else(|| panic!("`{after_source}` should be an assessed append: {change:?}"));

		assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Additive);
		assert_eq!(assessment.suggested_bump, BumpSeverity::Minor);
		assert_eq!(
			monochange_semver::semantic_change_severity(&change),
			BumpSeverity::Minor
		);
	}
}

/// Return every symbol change between two single-item sources.
fn diff_public_symbols_allow_empty(before_source: &str, after_source: &str) -> Vec<SemanticChange> {
	let key = |symbol: PublicSymbol| (symbol.item_kind.clone(), symbol.item_path.clone());
	let before = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: before_source.to_string(),
	})
	.unwrap_or_else(|error| panic!("before symbols should parse: {error}"))
	.into_iter()
	.map(|symbol| (key(symbol.clone()), symbol))
	.collect::<BTreeMap<_, _>>();
	let after = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: after_source.to_string(),
	})
	.unwrap_or_else(|error| panic!("after symbols should parse: {error}"))
	.into_iter()
	.map(|symbol| (key(symbol.clone()), symbol))
	.collect::<BTreeMap<_, _>>();

	diff_public_symbols(&before, &after)
}

/// Diff one constant (or static) between two single-item sources.
fn diff_one_constant(before_source: &str, after_source: &str) -> SemanticChange {
	let before = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: before_source.to_string(),
	})
	.unwrap_or_else(|error| panic!("before symbols should parse: {error}"));
	let after = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: after_source.to_string(),
	})
	.unwrap_or_else(|error| panic!("after symbols should parse: {error}"));
	let key = |symbol: PublicSymbol| (symbol.item_kind.clone(), symbol.item_path.clone());
	let before = before
		.into_iter()
		.map(|symbol| (key(symbol.clone()), symbol))
		.collect::<BTreeMap<_, _>>();
	let after = after
		.into_iter()
		.map(|symbol| (key(symbol.clone()), symbol))
		.collect::<BTreeMap<_, _>>();

	let mut changes = diff_public_symbols(&before, &after);

	assert_eq!(changes.len(), 1, "expected exactly one symbol change");
	changes.remove(0)
}

#[test]
fn manifest_helpers_cover_parse_failures_removed_entries_and_table_values() {
	let mut warnings = Vec::new();
	assert!(parse_manifest(Some("not = [valid"), Path::new("Cargo.toml"), &mut warnings).is_none());
	assert_eq!(warnings.len(), 1);

	let before = toml::from_str::<Value>(
		"[package]\nedition = \"2021\"\n\n[features]\ndefault = [\"cli\"]\n",
	)
	.unwrap_or_else(|error| panic!("parse before manifest: {error}"));
	let after = toml::from_str::<Value>("[package]\nedition = \"2024\"\n")
		.unwrap_or_else(|error| panic!("parse after manifest: {error}"));

	let before_metadata = extract_metadata_entries(&before);
	let after_metadata = extract_metadata_entries(&after);
	let changes = compare_manifest_entries(
		SemanticChangeCategory::Metadata,
		Path::new("Cargo.toml"),
		&before_metadata,
		&after_metadata,
	);

	assert!(changes.iter().any(|change| {
		change.item_path == "package.edition" && change.kind == SemanticChangeKind::Modified
	}));
	assert!(changes.iter().any(|change| {
		change.item_path == "feature.default" && change.kind == SemanticChangeKind::Removed
	}));
	let after_edition = after
		.get("package")
		.and_then(Value::as_table)
		.and_then(|package| package.get("edition"))
		.unwrap_or_else(|| panic!("expected package.edition"));
	assert_eq!(describe_manifest_value(after_edition), "2024");
	let before_default_feature = before
		.get("features")
		.and_then(Value::as_table)
		.and_then(|features| features.get("default"))
		.unwrap_or_else(|| panic!("expected features.default"));
	assert!(describe_manifest_value(before_default_feature).contains("cli"));
	let dependency_table = toml::from_str::<Value>("[dep]\nserde = \"1\"\n")
		.unwrap_or_else(|error| panic!("parse table manifest: {error}"));
	let dependency_value = dependency_table
		.get("dep")
		.unwrap_or_else(|| panic!("expected dep table"));

	assert!(describe_manifest_value(dependency_value).contains("serde=1"));
}

#[test]
fn api_snapshot_extracts_public_symbols_from_snapshot() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/repo/crates/core/Cargo.toml"),
		PathBuf::from("/repo"),
		None,
		monochange_core::PublishState::Public,
	);
	let snapshot = PackageSnapshot {
		label: "HEAD".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("src/lib.rs"),
			contents: "pub fn greet() {}\npub struct Greeter;".to_string(),
		}],
	};
	let context = PackageAnalysisContext {
		repo_root: Path::new("/repo"),
		package: &package,
		detection_level: DetectionLevel::Signature,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: Some(&snapshot),
	};

	let snapshot = api_snapshot(&context);

	assert_eq!(snapshot.package_id, "cargo:crates/core/Cargo.toml");
	assert_eq!(snapshot.analyzer_id, "cargo/public-api");
	assert!(snapshot.items.iter().any(|item| item.path == "greet"));
	assert!(snapshot.items.iter().any(|item| item.path == "Greeter"));
}
