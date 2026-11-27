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

#[test]
fn bonding_curve_inert_lint_attribute_and_docs_on_constant_produce_no_change() {
	let before = "pub const PINA_BONDING_CURVE_ID: Address = pina::address!(\"Curve1Gq\");";
	let after = concat!(
		"/// Program address for the bonding curve.\n",
		"/// See [broken link] for details.\n",
		"#[allow(rustdoc::broken_intra_doc_links, clippy::doc_lazy_continuation)]\n",
		"pub const PINA_BONDING_CURVE_ID: Address = pina::address!(\"Curve1Gq\");\n",
	);

	assert!(
		diff_public_symbols_allow_empty(before, after).is_empty(),
		"adding docs and #[allow(...)] on a public constant must not emit a finding"
	);
}

#[rstest::rstest]
#[case::constant_allow(
	"pub const LIMIT: usize = 3;",
	"#[allow(dead_code, clippy::all)] pub const LIMIT: usize = 3;"
)]
#[case::constant_warn(
	"pub const LIMIT: usize = 3;",
	"#[warn(missing_docs)] pub const LIMIT: usize = 3;"
)]
#[case::constant_deny(
	"pub const LIMIT: usize = 3;",
	"#[deny(unsafe_code)] pub const LIMIT: usize = 3;"
)]
#[case::constant_forbid(
	"pub const LIMIT: usize = 3;",
	"#[forbid(unsafe_code)] pub const LIMIT: usize = 3;"
)]
#[case::constant_expect(
	"pub const LIMIT: usize = 3;",
	"#[expect(clippy::doc_lazy_continuation, reason = \"generated\")] pub const LIMIT: usize = 3;"
)]
#[case::constant_rustfmt_skip(
	"pub const LIMIT: usize = 3;",
	"#[rustfmt::skip] pub const LIMIT: usize = 3;"
)]
#[case::constant_cfg_attr_allow(
	"pub const LIMIT: usize = 3;",
	"#[cfg_attr(not(test), allow(dead_code))] pub const LIMIT: usize = 3;"
)]
#[case::constant_cfg_attr_mixed(
	"#[cfg_attr(feature = \"ffi\", repr(C))] pub struct Item { pub a: u8 }",
	"#[cfg_attr(feature = \"ffi\", repr(C), allow(dead_code), expect(missing_docs))] pub struct Item { pub a: u8 }"
)]
#[case::enum_item_and_variant(
	"pub enum Mode { Fast { value: u8 }, Slow(u8) }",
	"#[allow(dead_code)] pub enum Mode { #[expect(missing_docs)] Fast { #[allow(dead_code)] value: u8 }, #[warn(deprecated)] Slow(#[allow(unused)] u8) }"
)]
#[case::function_item_and_param(
	"pub fn run(value: u8) -> u8 { value }",
	"#[allow(clippy::needless_pass_by_value)] #[expect(dead_code)] pub fn run(#[allow(unused_variables)] value: u8) -> u8 { value }"
)]
#[case::module_outer_inner_and_nested(
	"pub mod api { pub const X: u8 = 1; }",
	"#[allow(dead_code)] pub mod api { #![allow(missing_docs)] #[expect(clippy::all)] pub const X: u8 = 1; }"
)]
#[case::static_item(
	"pub static NAME: &str = \"core\";",
	"#[allow(dead_code)] #[cfg_attr(all(), warn(missing_docs))] pub static NAME: &str = \"core\";"
)]
#[case::struct_item_and_fields(
	"pub struct Item { pub value: u8 }",
	"#[allow(dead_code)] pub struct Item { #[expect(missing_docs)] pub value: u8 }"
)]
#[case::struct_tuple_field(
	"pub struct Item(pub u8);",
	"#[deny(missing_docs)] pub struct Item(#[allow(dead_code)] pub u8);"
)]
#[case::trait_item_and_members(
	"pub trait Renderer { const LIMIT: u8 = 3; type Output; fn render(&self) {} }",
	"#[allow(dead_code)] pub trait Renderer { #[expect(missing_docs)] const LIMIT: u8 = 3; #[allow(dead_code)] type Output; #[allow(clippy::all)] fn render(&self) {} }"
)]
#[case::type_alias(
	"pub type Name = String;",
	"#[allow(dead_code)] #[forbid(unsafe_code)] pub type Name = String;"
)]
#[case::union_item_and_field(
	"pub union Number { pub value: u32 }",
	"#[allow(dead_code)] pub union Number { #[expect(missing_docs)] pub value: u32 }"
)]
#[case::reexport(
	"pub use crate::helpers::render;",
	"#[allow(unused_imports)] #[cfg_attr(not(test), expect(unused_imports))] pub use crate::helpers::render;"
)]
#[case::track_caller_on_fn("pub fn run() {}", "#[track_caller] pub fn run() {}")]
#[case::track_caller_on_trait_method(
	"pub trait Checker { fn check(&self) {} }",
	"pub trait Checker { #[track_caller] fn check(&self) {} }"
)]
fn inert_lint_attributes_do_not_change_signatures(#[case] plain: &str, #[case] annotated: &str) {
	assert!(
		diff_public_symbols_allow_empty(plain, annotated).is_empty(),
		"adding inert attribute should produce no diff:\nplain: {plain}\nannotated: {annotated}"
	);
	assert!(
		diff_public_symbols_allow_empty(annotated, plain).is_empty(),
		"removing inert attribute should produce no diff:\nplain: {plain}\nannotated: {annotated}"
	);
}

#[rstest::rstest]
#[case::tracing_instrument_on_fn(
	"pub fn fetch(id: u64) -> u64 { id }",
	"#[tracing::instrument(skip(id))] pub fn fetch(id: u64) -> u64 { id }"
)]
#[case::bare_instrument_on_fn(
	"pub fn fetch(id: u64) -> u64 { id }",
	"#[instrument] pub fn fetch(id: u64) -> u64 { id }"
)]
#[case::tauri_command_on_fn(
	"pub fn greet(name: String) -> String { name }",
	"#[tauri::command] pub fn greet(name: String) -> String { name }"
)]
#[case::napi_on_fn(
	"pub fn add(a: i32, b: i32) -> i32 { a + b }",
	"#[napi] pub fn add(a: i32, b: i32) -> i32 { a + b }"
)]
#[case::cold_on_fn("pub fn slow_path() {}", "#[cold] pub fn slow_path() {}")]
#[case::cfg_attr_tracing_instrument_on_fn(
	"pub fn work() {}",
	"#[cfg_attr(feature = \"tracing\", tracing::instrument)] pub fn work() {}"
)]
#[case::clippy_too_many_arguments_on_fn(
	"pub fn many(a: u8, b: u8) -> u8 { a + b }",
	"#[clippy::too_many_arguments] pub fn many(a: u8, b: u8) -> u8 { a + b }"
)]
#[case::clippy_msrv_on_fn("pub fn work() {}", "#[clippy::msrv = \"1.70\"] pub fn work() {}")]
#[case::reordered_attrs_on_fn(
	"#[inline] #[must_use] pub fn compute() -> u8 { 1 }",
	"#[must_use] #[inline] pub fn compute() -> u8 { 1 }"
)]
#[case::cfg_twin_fns_reordered(
	"#[cfg(unix)] pub fn path_sep() -> char { '/' }\n#[cfg(windows)] pub fn path_sep() -> char { '\\\\' }",
	"#[cfg(windows)] pub fn path_sep() -> char { '\\\\' }\n#[cfg(unix)] pub fn path_sep() -> char { '/' }"
)]
#[case::macro_export_arm_added(
	"#[macro_export] macro_rules! make { () => { 1 }; }",
	"#[macro_export] macro_rules! make { () => { 1 }; ($x:expr) => { $x }; }"
)]
#[case::doc_hidden_removed_from_struct(
	"#[doc(hidden)] pub struct Internal;",
	"pub struct Internal;"
)]
#[case::doc_hidden_removed_from_fn("#[doc(hidden)] pub fn internal() {}", "pub fn internal() {}")]
#[case::doc_hidden_removed_from_trait_method(
	"pub trait Service { #[doc(hidden)] fn call(&self) {} }",
	"pub trait Service { fn call(&self) {} }"
)]
fn main_compatible_non_breaking_changes_produce_no_finding(
	#[case] before: &str,
	#[case] after: &str,
) {
	let changes = diff_public_symbols_allow_empty(before, after);
	assert!(
		changes.is_empty(),
		"expected no finding for:\nbefore: {before}\nafter: {after}\ngot: {changes:?}"
	);
}

#[rstest::rstest]
#[case::repr_on_struct(
	"pub struct Item { pub a: u8 }",
	"#[repr(C)] pub struct Item { pub a: u8 }"
)]
#[case::repr_on_enum("pub enum Mode { A, B }", "#[repr(u8)] pub enum Mode { A, B }")]
#[case::repr_on_union(
	"pub union Data { pub a: u32, pub b: f32 }",
	"#[repr(C)] pub union Data { pub a: u32, pub b: f32 }"
)]
#[case::non_exhaustive_on_struct(
	"pub struct Item { pub a: u8 }",
	"#[non_exhaustive] pub struct Item { pub a: u8 }"
)]
#[case::non_exhaustive_on_enum("pub enum Mode { A }", "#[non_exhaustive] pub enum Mode { A }")]
#[case::cfg_on_struct("pub struct Item;", "#[cfg(feature = \"extra\")] pub struct Item;")]
#[case::derive_removed_on_struct(
	"#[derive(Debug, Clone)] pub struct Item;",
	"#[derive(Debug)] pub struct Item;"
)]
#[case::no_mangle_on_static("pub static FLAG: u8 = 1;", "#[no_mangle] pub static FLAG: u8 = 1;")]
#[case::export_name_on_static(
	"pub static FLAG: u8 = 1;",
	"#[export_name = \"c_flag\"] pub static FLAG: u8 = 1;"
)]
#[case::link_name_in_module(
	"pub mod ffi { unsafe extern \"C\" { pub fn ext(); } }",
	"pub mod ffi { unsafe extern \"C\" { #[unsafe(link_name = \"c_ext\")] pub fn ext(); } }"
)]
#[case::proc_macro_on_struct(
	"pub struct Config { pub retries: u32 }",
	"#[serde(rename_all = \"camelCase\")] pub struct Config { pub retries: u32 }"
)]
#[case::mixed_compatible_and_breaking_attrs_on_struct(
	"pub struct Item { pub a: u8 }",
	"#[must_use] #[repr(C)] pub struct Item { pub a: u8 }"
)]
#[case::slice_append_with_breaking_attr_stays_major(
	"pub const LIST: &[u8] = &[1, 2];",
	"#[cfg(feature = \"extra\")] pub const LIST: &[u8] = &[1, 2, 3];"
)]
#[case::trait_defaulted_method_with_breaking_trait_attr_stays_major(
	"pub trait Runner { fn run(&self); }",
	"#[cfg(feature = \"extra\")] pub trait Runner { fn run(&self); fn helper(&self) {} }"
)]
fn breaking_non_inert_attributes_remain_major(#[case] before: &str, #[case] after: &str) {
	let changes = diff_public_symbols_allow_empty(before, after);
	assert_eq!(
		changes.len(),
		1,
		"expected one breaking change for:\nbefore: {before}\nafter: {after}\ngot: {changes:?}"
	);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one change"));
	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert_eq!(
		monochange_semver::semantic_change_severity(change),
		BumpSeverity::Major,
		"expected major severity for {change:?}"
	);
}

#[rstest::rstest]
#[case::inline_on_trait_default_method(
	"pub trait Fast { fn fast(&self) -> u8 { 1 } }",
	"pub trait Fast { #[inline] fn fast(&self) -> u8 { 1 } }",
	BumpSeverity::Patch
)]
#[case::inline_always_in_cfg_attr_on_trait_method(
	"pub trait Fast { fn fast(&self) -> u8 { 1 } }",
	"pub trait Fast { #[cfg_attr(not(debug_assertions), inline(always))] fn fast(&self) -> u8 { 1 } }",
	BumpSeverity::Patch
)]
#[case::must_use_on_struct(
	"pub struct Guard;",
	"#[must_use = \"hold the guard\"] pub struct Guard;",
	BumpSeverity::Patch
)]
#[case::deprecated_added_on_struct(
	"pub struct Old;",
	"#[deprecated] pub struct Old;",
	BumpSeverity::Minor
)]
#[case::deprecated_added_with_note_on_struct(
	"pub struct Old;",
	"#[deprecated(since = \"1.2.0\", note = \"use New\")] pub struct Old;",
	BumpSeverity::Minor
)]
#[case::deprecated_added_on_trait(
	"pub trait Old { fn run(&self); }",
	"#[deprecated] pub trait Old { fn run(&self); }",
	BumpSeverity::Minor
)]
#[case::deprecated_added_on_existing_trait_method(
	"pub trait Runner { fn run(&self) {} }",
	"pub trait Runner { #[deprecated] fn run(&self) {} }",
	BumpSeverity::Minor
)]
#[case::cfg_attr_deprecated_added_on_struct(
	"pub struct Old;",
	"#[cfg_attr(feature = \"legacy\", deprecated)] pub struct Old;",
	BumpSeverity::Minor
)]
#[case::cfg_attr_deprecated_promoted_to_unconditional_on_struct(
	"#[cfg_attr(feature = \"legacy\", deprecated)] pub struct Old;",
	"#[deprecated] pub struct Old;",
	BumpSeverity::Minor
)]
#[case::deprecated_note_edited_on_struct(
	"#[deprecated(note = \"old\")] pub struct Old;",
	"#[deprecated(note = \"new\")] pub struct Old;",
	BumpSeverity::Patch
)]
#[case::deprecated_since_added_on_struct(
	"#[deprecated(note = \"old\")] pub struct Old;",
	"#[deprecated(since = \"1.2.0\", note = \"old\")] pub struct Old;",
	BumpSeverity::Patch
)]
#[case::deprecated_note_added_to_bare_deprecated_on_trait(
	"#[deprecated] pub trait Old { fn run(&self); }",
	"#[deprecated(note = \"use New\")] pub trait Old { fn run(&self); }",
	BumpSeverity::Patch
)]
#[case::deprecated_note_edited_on_trait_method(
	"pub trait Runner { #[deprecated(note = \"old\")] fn run(&self) {} }",
	"pub trait Runner { #[deprecated(note = \"new\")] fn run(&self) {} }",
	BumpSeverity::Patch
)]
#[case::cfg_attr_deprecated_note_edited_on_struct(
	"#[cfg_attr(feature = \"legacy\", deprecated(note = \"old\"))] pub struct Old;",
	"#[cfg_attr(feature = \"legacy\", deprecated(note = \"new\"))] pub struct Old;",
	BumpSeverity::Patch
)]
#[case::deprecated_removed_on_struct(
	"#[deprecated] pub struct Old;",
	"pub struct Old;",
	BumpSeverity::Patch
)]
#[case::reordered_compatible_attrs_on_struct(
	"#[must_use] #[deprecated] pub struct Guard;",
	"#[deprecated] #[must_use] pub struct Guard;",
	BumpSeverity::Patch
)]
fn modelled_compatible_attributes_produce_non_breaking_findings(
	#[case] before: &str,
	#[case] after: &str,
	#[case] expected_bump: BumpSeverity,
) {
	let changes = diff_public_symbols_allow_empty(before, after);
	assert_eq!(
		changes.len(),
		1,
		"expected one compatible finding for:\nbefore: {before}\nafter: {after}\ngot: {changes:?}"
	);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one change"));
	let assessment = change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("expected explicit assessment on {change:?}"));
	assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Compatible);
	assert_eq!(assessment.suggested_bump, expected_bump);
	assert_eq!(assessment.confidence, ApiConfidence::Medium);
	assert_eq!(
		assessment.evidence.completeness,
		SemanticAnalysisCompleteness::Partial
	);
	assert_eq!(
		monochange_semver::semantic_change_severity(change),
		expected_bump
	);
}

#[test]
fn slice_append_with_inert_attribute_remains_additive() {
	let change = diff_one_constant(
		"pub const LIST: &[u8] = &[1, 2];\n",
		"#[allow(dead_code)] pub const LIST: &[u8] = &[1, 2, 3];\n",
	);
	let assessment = change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("expected slice append assessment: {change:?}"));
	assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Additive);
	assert_eq!(assessment.suggested_bump, BumpSeverity::Minor);
}

#[test]
fn pina_parse_accounts_default_doc_hidden_method_is_additive() {
	let before = concat!(
		"pub trait ParseAccounts<'a>: Sized {\n",
		"    fn parse_accounts(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError>;\n",
		"}\n",
	);
	let after = concat!(
		"pub trait ParseAccounts<'a>: Sized {\n",
		"    fn parse_accounts(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError>;\n",
		"    #[doc(hidden)]\n",
		"    fn __try_from_account_infos(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError> {\n",
		"        Self::parse_accounts(accounts)\n",
		"    }\n",
		"}\n",
	);

	let changes = diff_public_symbols_allow_empty(before, after);
	assert_eq!(changes.len(), 1);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one change"));
	let assessment = change.assessment.as_ref().unwrap_or_else(|| {
		panic!("expected additive assessment for defaulted trait method: {change:?}")
	});
	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Additive);
	assert_eq!(assessment.suggested_bump, BumpSeverity::Minor);
	assert_eq!(assessment.confidence, ApiConfidence::Medium);
	assert_eq!(
		assessment.evidence.completeness,
		SemanticAnalysisCompleteness::Partial
	);
	assert_eq!(
		monochange_semver::semantic_change_severity(change),
		BumpSeverity::Minor
	);
}

#[rstest::rstest]
#[case::new_defaulted_ref_self_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn run_twice(&self) { self.run(); self.run(); } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_mut_self_method(
	"pub trait Runner { fn run(&mut self); }",
	"pub trait Runner { fn run(&mut self); fn reset(&mut self) {} }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_box_self_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn finish(self: Box<Self>) {} }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_lifetime_generic_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn label<'a>(&'a self) -> &'a str { \"\" } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_method_with_self_assoc_projection(
	"pub trait Producer { type Output; fn produce(&self) -> Self::Output; }",
	"pub trait Producer { type Output; fn produce(&self) -> Self::Output; fn passthrough(&self, x: Self::Output) -> Self::Output { x } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_method_with_qualified_self_projection(
	"pub trait Sub: Super { fn run(&self); }",
	"pub trait Sub: Super { fn run(&self); fn map_out(&self, x: <Self as Super>::Out) -> <Self as Super>::Out { x } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_generic_method_with_where_self_sized(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn map<T>(&self, f: impl Fn(u8) -> T) -> T where Self: Sized { f(1) } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_async_method_with_where_self_sized(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); async fn run_async(&self) where Self: Sized { self.run(); } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_receiverless_fn_with_where_self_sized(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn helper() -> u8 where Self: Sized { 1 } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_self_returning_method_with_where_self_sized(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn dup(&self) -> Option<Self> where Self: Sized { None } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_assoc_type_with_where_self_sized_nightly(
	"pub trait Codec { fn encode(&self); }",
	"pub trait Codec { fn encode(&self); type Metadata = () where Self: Sized; }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_generic_method_on_sized_trait(
	"pub trait Runner: Sized { fn run(&self); }",
	"pub trait Runner: Sized { fn run(&self); fn map<T>(&self, f: impl Fn(u8) -> T) -> T { f(1) } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_async_method_on_where_self_sized_trait(
	"pub trait Runner where Self: Sized { fn run(&self); }",
	"pub trait Runner where Self: Sized { fn run(&self); async fn run_async(&self) { self.run(); } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_receiverless_fn_on_already_non_dyn_trait(
	"pub trait Parser { fn parse(input: &str) -> Self; }",
	"pub trait Parser { fn parse(input: &str) -> Self; fn parse_default() -> Self { Self::parse(\"\") } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_self_arg_method_on_already_non_dyn_trait(
	"pub trait Parser { fn parse(input: &str) -> Self; }",
	"pub trait Parser { fn parse(input: &str) -> Self; fn same(&self, _other: &Self) -> bool { false } }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_const_on_already_non_dyn_trait(
	"pub trait Config: Sized { fn name(&self) -> &str; }",
	"pub trait Config: Sized { fn name(&self) -> &str; const MAX_RETRIES: usize = 3; }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_const_on_trait_with_existing_const(
	"pub trait Config { const VERSION: u8; fn name(&self) -> &str; }",
	"pub trait Config { const VERSION: u8; fn name(&self) -> &str; const MAX_RETRIES: usize = 3; }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::new_defaulted_type_on_sized_trait_nightly(
	"pub trait Codec: Sized { fn encode(&self); }",
	"pub trait Codec: Sized { fn encode(&self); type Metadata = (); }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::required_method_gains_default(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self) {} }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::required_const_gains_default(
	"pub trait Config { const LIMIT: usize; }",
	"pub trait Config { const LIMIT: usize = 8; }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::required_type_gains_default_nightly(
	"pub trait Codec { type Error; }",
	"pub trait Codec { type Error = std::io::Error; }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::defaulted_method_with_trait_deprecated_attr(
	"pub trait Runner { fn run(&self); }",
	"#[deprecated] pub trait Runner { fn run(&self); fn helper(&self) {} }",
	SemanticAnalysisOutcome::Additive,
	BumpSeverity::Minor
)]
#[case::default_method_body_edit_only(
	"pub trait Runner { fn count(&self) -> usize { 1 } }",
	"pub trait Runner { fn count(&self) -> usize { 2 } }",
	SemanticAnalysisOutcome::Compatible,
	BumpSeverity::Patch
)]
#[case::trait_member_reorder_only(
	"pub trait Runner { fn a(&self); fn b(&self); }",
	"pub trait Runner { fn b(&self); fn a(&self); }",
	SemanticAnalysisOutcome::Compatible,
	BumpSeverity::Patch
)]
fn trait_non_breaking_deltas_are_assessed_accurately(
	#[case] before: &str,
	#[case] after: &str,
	#[case] expected_outcome: SemanticAnalysisOutcome,
	#[case] expected_bump: BumpSeverity,
) {
	let changes = diff_public_symbols_allow_empty(before, after);
	assert_eq!(changes.len(), 1, "expected one change: {changes:?}");
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one change"));
	let assessment = change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("expected explicit assessment on {change:?}"));
	assert_eq!(assessment.outcome, expected_outcome);
	assert_eq!(assessment.suggested_bump, expected_bump);
	assert_eq!(assessment.confidence, ApiConfidence::Medium);
	assert_eq!(
		assessment.evidence.completeness,
		SemanticAnalysisCompleteness::Partial
	);
	assert_eq!(
		monochange_semver::semantic_change_severity(change),
		expected_bump
	);
}

#[rstest::rstest]
#[case::defaulted_type_generic_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn map<T>(&self, f: impl Fn(u8) -> T) -> T { f(1) } }",
	"map"
)]
#[case::defaulted_const_generic_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn fixed<const N: usize>(&self) -> usize { N } }",
	"fixed"
)]
#[case::defaulted_impl_trait_arg_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn apply(&self, f: impl Fn(u8) -> u8) -> u8 { f(1) } }",
	"apply"
)]
#[case::defaulted_rpitit_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn items(&self) -> impl Iterator<Item = u8> { core::iter::empty() } }",
	"items"
)]
#[case::defaulted_async_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); async fn go(&self) {} }",
	"go"
)]
#[case::defaulted_const_fn_nightly(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); const fn tag(&self) -> u8 { 1 } }",
	"tag"
)]
#[case::defaulted_receiverless_fn(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn helper() -> u8 { 1 } }",
	"helper"
)]
#[case::defaulted_method_taking_self_by_value(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn merge(&self, _other: Self) -> u8 { 0 } }",
	"merge"
)]
#[case::defaulted_method_taking_ref_self(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn eq_other(&self, _other: &Self) -> bool { false } }",
	"eq_other"
)]
#[case::defaulted_method_returning_self(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn dup(&self) -> Self { loop {} } }",
	"dup"
)]
#[case::defaulted_method_returning_option_self(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn maybe_dup(&self) -> Option<Self> { None } }",
	"maybe_dup"
)]
#[case::defaulted_method_with_where_self_non_sized(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn check_send(&self) where Self: Send {} }",
	"check_send"
)]
#[case::defaulted_method_with_self_inside_assoc_generic_arg(
	"pub trait Producer { type Out; fn run(&self); }",
	"pub trait Producer { type Out; fn run(&self); fn nested(&self, _x: Self::Out<Self>) {} }",
	"nested"
)]
#[case::defaulted_method_with_self_inside_qualified_projection_arg(
	"pub trait Sub: Super { fn run(&self); }",
	"pub trait Sub: Super { fn run(&self); fn nested(&self, _x: <Self as Super<Self>>::Out) {} }",
	"nested"
)]
#[case::defaulted_assoc_const_on_dyn_trait(
	"pub trait Config { fn name(&self) -> &str; }",
	"pub trait Config { fn name(&self) -> &str; const MAX_RETRIES: usize = 3; }",
	"MAX_RETRIES"
)]
#[case::defaulted_assoc_type_on_dyn_trait_nightly(
	"pub trait Codec { fn encode(&self); }",
	"pub trait Codec { fn encode(&self); type Metadata = (); }",
	"Metadata"
)]
#[case::defaulted_gat_on_dyn_trait_nightly(
	"pub trait Codec { fn encode(&self); }",
	"pub trait Codec { fn encode(&self); type Item<T> = T; }",
	"Item"
)]
fn trait_defaulted_additions_breaking_dyn_compatibility_stay_major(
	#[case] before: &str,
	#[case] after: &str,
	#[case] expected_member: &str,
) {
	let changes = diff_public_symbols_allow_empty(before, after);
	assert_eq!(
		changes.len(),
		1,
		"expected one breaking change for dyn-incompatible addition: {changes:?}"
	);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one change"));
	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert!(
		change.summary.contains("dyn-compatibility"),
		"summary should explain dyn-compatibility break: {}",
		change.summary
	);
	assert!(
		change.summary.contains(expected_member),
		"summary should name offending member `{expected_member}`: {}",
		change.summary
	);
	let assessment = change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("expected explicit breaking assessment: {change:?}"));
	assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Breaking);
	assert_eq!(assessment.suggested_bump, BumpSeverity::Major);
	assert_eq!(
		monochange_semver::semantic_change_severity(change),
		BumpSeverity::Major
	);
}

#[rstest::rstest]
#[case::new_required_method(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&self); fn stop(&self); }"
)]
#[case::new_required_const(
	"pub trait Config { fn name(&self); }",
	"pub trait Config { fn name(&self); const LIMIT: usize; }"
)]
#[case::new_required_type(
	"pub trait Codec { fn encode(&self); }",
	"pub trait Codec { fn encode(&self); type Output; }"
)]
#[case::changed_method_signature(
	"pub trait Runner { fn run(&self, count: u32) -> bool; }",
	"pub trait Runner { fn run(&self, count: u64) -> bool; }"
)]
#[case::changed_method_receiver(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner { fn run(&mut self); }"
)]
#[case::removed_method(
	"pub trait Runner { fn run(&self); fn stop(&self) {} }",
	"pub trait Runner { fn run(&self); }"
)]
#[case::removed_default_body(
	"pub trait Runner { fn run(&self) {} }",
	"pub trait Runner { fn run(&self); }"
)]
#[case::changed_default_const_value(
	"pub trait Config { const LIMIT: usize = 4; }",
	"pub trait Config { const LIMIT: usize = 8; }"
)]
#[case::removed_default_const_value(
	"pub trait Config { const LIMIT: usize = 4; }",
	"pub trait Config { const LIMIT: usize; }"
)]
#[case::changed_default_type(
	"pub trait Codec { type Output = u8; }",
	"pub trait Codec { type Output = u16; }"
)]
#[case::removed_default_type(
	"pub trait Codec { type Output = u8; }",
	"pub trait Codec { type Output; }"
)]
#[case::added_supertrait(
	"pub trait Runner { fn run(&self); }",
	"pub trait Runner: Clone { fn run(&self); }"
)]
#[case::removed_supertrait(
	"pub trait Runner: Clone { fn run(&self); }",
	"pub trait Runner { fn run(&self); }"
)]
#[case::changed_generics_and_bounds(
	"pub trait Mapper<T> { fn map(&self, input: T); }",
	"pub trait Mapper<T: Clone> { fn map(&self, input: T); }"
)]
#[case::changed_where_clause(
	"pub trait Mapper<T> { fn map(&self, input: T); }",
	"pub trait Mapper<T> where T: Send { fn map(&self, input: T); }"
)]
#[case::added_unsafe(
	"pub trait Marker { fn id(&self) -> u8; }",
	"pub unsafe trait Marker { fn id(&self) -> u8; }"
)]
#[case::removed_unsafe(
	"pub unsafe trait Marker { fn id(&self) -> u8; }",
	"pub trait Marker { fn id(&self) -> u8; }"
)]
#[case::sealed_supertrait_added(
	"pub trait Service { fn call(&self); }",
	"pub trait Service: private::Sealed { fn call(&self); }"
)]
#[case::sealed_supertrait_removed(
	"pub trait Service: private::Sealed { fn call(&self); }",
	"pub trait Service { fn call(&self); }"
)]
#[case::sealed_where_bound_added(
	"pub trait Service { fn call(&self); }",
	"pub trait Service where Self: private::Sealed { fn call(&self); }"
)]
#[case::sealed_required_method_added(
	"pub trait Service { fn call(&self); }",
	"pub trait Service { fn call(&self); #[doc(hidden)] fn __sealed(&self, _: private::Token); }"
)]
#[case::mixed_defaulted_and_required_methods(
	"pub trait Service { fn call(&self); }",
	"pub trait Service { fn call(&self); fn ok(&self) {} fn required(&self); }"
)]
#[case::opaque_macro_item_added(
	"pub trait Service { fn call(&self); }",
	"pub trait Service { fn call(&self); define_methods!(); }"
)]
#[case::duplicate_cfg_gated_methods(
	"pub trait Service { fn call(&self); }",
	"pub trait Service { fn call(&self); #[cfg(feature = \"a\")] fn extra(&self) {} #[cfg(feature = \"b\")] fn extra(&self) {} }"
)]
fn trait_breaking_changes_remain_major(#[case] before: &str, #[case] after: &str) {
	let changes = diff_public_symbols_allow_empty(before, after);
	assert_eq!(
		changes.len(),
		1,
		"expected one breaking change: {changes:?}"
	);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one change"));
	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert_eq!(
		monochange_semver::semantic_change_severity(change),
		BumpSeverity::Major,
		"expected major severity for {change:?}"
	);
}
