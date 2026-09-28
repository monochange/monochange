use monochange_core::AnalyzedFileChange;
use monochange_core::FileChangeKind;

use super::*;

#[test]
fn module_prefix_for_root_library_file_is_empty() {
	assert!(module_prefix_for_file(Path::new("src/lib.rs")).is_empty());
}

#[test]
fn analyzer_constructors_keep_the_default_analyzer_available() {
	assert_eq!(semantic_analyzer().analyzer_id(), "cargo/public-api");
	assert_eq!(
		CargoSemanticAnalyzer::default().analyzer_id(),
		"cargo/public-api"
	);
}

#[test]
fn complete_semver_results_replace_only_conservative_public_api_changes() {
	let mut changes = vec![
		SemanticChange::new(
			SemanticChangeCategory::PublicApi,
			SemanticChangeKind::Removed,
			"function",
			"removed",
			"removed",
			"src/lib.rs",
		),
		SemanticChange::new(
			SemanticChangeCategory::PublicApi,
			SemanticChangeKind::Added,
			"function",
			"added",
			"added",
			"src/lib.rs",
		),
		SemanticChange::new(
			SemanticChangeCategory::Metadata,
			SemanticChangeKind::Modified,
			"manifest",
			"edition",
			"changed",
			"Cargo.toml",
		),
	];
	let matrix = SemanticChange::new(
		SemanticChangeCategory::PublicApi,
		SemanticChangeKind::Modified,
		"compatibility_matrix",
		"configured_matrix",
		"compatible",
		"Cargo.toml",
	);

	merge_semver_analysis(
		&mut changes,
		Some(CargoSemverAnalysis {
			change: matrix,
			replace_syntax_changes: true,
		}),
	);

	assert_eq!(changes.len(), 3);
	assert!(changes.iter().any(|change| change.item_path == "added"));
	assert!(changes.iter().any(|change| change.item_path == "edition"));
	assert!(
		changes
			.iter()
			.any(|change| change.item_path == "configured_matrix")
	);
	assert!(!changes.iter().any(|change| change.item_path == "removed"));
	let previous_len = changes.len();
	merge_semver_analysis(&mut changes, None);
	assert_eq!(changes.len(), previous_len);
}

#[test]
fn module_prefix_for_nested_module_tracks_path_components() {
	assert_eq!(
		module_prefix_for_file(Path::new("src/api/render.rs")),
		vec!["api".to_string(), "render".to_string()]
	);
	assert_eq!(
		module_prefix_for_file(Path::new("src/api/mod.rs")),
		vec!["api".to_string()]
	);
}

#[test]
fn analyze_manifest_change_reports_dependency_and_feature_diffs() {
	let change = AnalyzedFileChange {
		path: PathBuf::from("crates/core/Cargo.toml"),
		package_path: PathBuf::from("Cargo.toml"),
		kind: FileChangeKind::Modified,
		before_contents: Some(
			"[package]\nname = \"core\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n\n[features]\ndefault = []\n"
				.to_string(),
		),
		after_contents: Some(
			"[package]\nname = \"core\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\ntracing = \"0.1\"\n\n[features]\ndefault = [\"cli\"]\ncli = []\n"
				.to_string(),
		),
	};
	let mut warnings = Vec::new();
	let changes = analyze_manifest_change(&change, &mut warnings);

	assert!(warnings.is_empty());
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Dependency
			&& change.item_path == "tracing"
			&& change.kind == SemanticChangeKind::Added
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Metadata
			&& change.item_path == "package.edition"
			&& change.kind == SemanticChangeKind::Modified
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Metadata
			&& change.item_path == "feature.cli"
			&& change.kind == SemanticChangeKind::Added
	}));
}

#[test]
fn collect_public_symbols_finds_public_items() {
	let file = PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: concat!(
			"pub struct Greeter;\n",
			"pub fn greet() {}\n",
			"pub mod api { pub fn render() {} }\n",
			"fn helper() {}\n",
		)
		.to_string(),
	};

	let symbols = collect_public_symbols(&file)
		.unwrap_or_else(|error| panic!("symbol extraction should succeed: {error}"));

	assert!(symbols.iter().any(|symbol| symbol.item_path == "Greeter"));
	assert!(symbols.iter().any(|symbol| symbol.item_path == "greet"));
	assert!(symbols.iter().any(|symbol| symbol.item_path == "api"));
	assert!(
		symbols
			.iter()
			.any(|symbol| symbol.item_path == "api::render")
	);
	assert!(!symbols.iter().any(|symbol| symbol.item_path == "helper"));
}

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

#[test]
fn module_prefix_and_symbol_diff_cover_root_removed_and_unchanged_paths() {
	assert!(module_prefix_for_file(Path::new("src/lib.rs")).is_empty());
	assert!(module_prefix_for_file(Path::new("src/main.rs")).is_empty());
	assert!(module_prefix_for_file(Path::new("lib.rs")).is_empty());

	let before = BTreeMap::from([
		(
			("function".to_string(), "greet".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "greet".to_string(),
				signature: "pub fn greet()".to_string(),
				initializer: None,
				file_path: PathBuf::from("src/lib.rs"),
			},
		),
		(
			("struct".to_string(), "Greeter".to_string()),
			PublicSymbol {
				item_kind: "struct".to_string(),
				item_path: "Greeter".to_string(),
				signature: "pub struct Greeter;".to_string(),
				initializer: None,
				file_path: PathBuf::from("src/lib.rs"),
			},
		),
	]);
	let after = BTreeMap::from([
		(
			("function".to_string(), "greet".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "greet".to_string(),
				signature: "pub fn greet(name: &str)".to_string(),
				initializer: None,
				file_path: PathBuf::from("src/lib.rs"),
			},
		),
		(
			("constant".to_string(), "LIMIT".to_string()),
			PublicSymbol {
				item_kind: "constant".to_string(),
				item_path: "LIMIT".to_string(),
				signature: "pub const LIMIT: usize = 3;".to_string(),
				initializer: None,
				file_path: PathBuf::from("src/lib.rs"),
			},
		),
	]);

	let changes = diff_public_symbols(&before, &after);

	assert!(
		changes
			.iter()
			.any(|change| change.kind == SemanticChangeKind::Modified)
	);
	assert!(
		changes
			.iter()
			.any(|change| change.kind == SemanticChangeKind::Removed)
	);
	assert!(changes.iter().all(|change| {
		change.summary.contains("added")
			|| change.summary.contains("modified")
			|| change.summary.contains("removed")
	}));
}

#[test]
fn appending_to_a_public_slice_is_assessed_as_additive() {
	let before = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: concat!(
			"pub const LINT_NAMES: &[&str] = &[\"a\", \"b\"];\n",
			"pub static LINTS: &[&str] = &[\"a\", \"b\"];\n",
			"pub const VALUES: &[u8] = &[1, 2];\n",
		)
		.to_string(),
	})
	.unwrap_or_else(|error| panic!("before symbols should parse: {error}"));
	let after = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("src/lib.rs"),
		contents: concat!(
			"pub const LINT_NAMES: &[&str] = &[\"a\", \"b\", \"c\"];\n",
			"pub static LINTS: &[&str] = &[\"a\", \"b\", \"c\"];\n",
			"pub const VALUES: &[u8] = &[1, 2];\n",
		)
		.to_string(),
	})
	.unwrap_or_else(|error| panic!("after symbols should parse: {error}"));
	let before = before
		.into_iter()
		.map(|symbol| ((symbol.item_kind.clone(), symbol.item_path.clone()), symbol))
		.collect::<BTreeMap<_, _>>();
	let after = after
		.into_iter()
		.map(|symbol| ((symbol.item_kind.clone(), symbol.item_path.clone()), symbol))
		.collect::<BTreeMap<_, _>>();

	let changes = diff_public_symbols(&before, &after);

	assert_eq!(changes.len(), 2);
	for change in &changes {
		assert_eq!(change.kind, SemanticChangeKind::Modified);
		let assessment = change
			.assessment
			.as_ref()
			.unwrap_or_else(|| panic!("slice append should carry an assessment: {change:?}"));
		assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Additive);
		assert_eq!(assessment.suggested_bump, BumpSeverity::Minor);
		assert_eq!(assessment.confidence, ApiConfidence::High);
		assert!(
			assessment.evidence.coverage.contains("appends elements"),
			"coverage note should name the append rule: {}",
			assessment.evidence.coverage
		);
	}
	assert_eq!(
		monochange_semver::semantic_change_severity(&changes[0]),
		BumpSeverity::Minor
	);
}

#[test]
fn slice_modifications_that_are_not_pure_appends_stay_conservative() {
	let cases = [
		// A removal is not an append.
		(
			"pub const NAMES: &[&str] = &[\"a\", \"b\", \"c\"];\n",
			"pub const NAMES: &[&str] = &[\"a\", \"b\"];\n",
		),
		// Reordering keeps the same elements but changes observable order.
		(
			"pub const NAMES: &[&str] = &[\"a\", \"b\"];\n",
			"pub const NAMES: &[&str] = &[\"b\", \"a\"];\n",
		),
		// Editing an element is not an append.
		(
			"pub const NAMES: &[&str] = &[\"a\", \"b\"];\n",
			"pub const NAMES: &[&str] = &[\"a\", \"z\"];\n",
		),
		// An append after a removal is not a pure append.
		(
			"pub const NAMES: &[&str] = &[\"a\", \"b\"];\n",
			"pub const NAMES: &[&str] = &[\"b\", \"c\"];\n",
		),
	];

	for (before_source, after_source) in cases {
		let change = diff_one_constant(before_source, after_source);

		assert_eq!(change.kind, SemanticChangeKind::Modified);
		assert!(
			change.assessment.is_none(),
			"`{before_source}` to `{after_source}` must not be assessed additive: {change:?}"
		);
		assert_eq!(
			monochange_semver::semantic_change_severity(&change),
			BumpSeverity::Major,
			"`{before_source}` to `{after_source}` must stay conservative"
		);
	}
}

#[test]
fn slice_append_assessment_requires_a_slice_literal() {
	// A scalar constant is never treated as a slice append.
	let change = diff_one_constant(
		"pub const LIMIT: usize = 3;\n",
		"pub const LIMIT: usize = 4;\n",
	);

	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert!(change.assessment.is_none());
	assert_eq!(
		monochange_semver::semantic_change_severity(&change),
		BumpSeverity::Major
	);
}

#[test]
fn slice_append_detection_ignores_non_literal_initializers() {
	// A path initializer has no element sequence to compare.
	let change = diff_one_constant(
		"pub const NAMES: &[&str] = NAMES_V1;\n",
		"pub const NAMES: &[&str] = NAMES_V2;\n",
	);

	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert!(change.assessment.is_none());
	assert_eq!(
		monochange_semver::semantic_change_severity(&change),
		BumpSeverity::Major
	);
}

#[test]
fn slice_append_requires_a_growing_literal_sequence() {
	// Both endpoints are slice literals, but the after literal is shorter.
	let change = diff_one_constant(
		"pub const NAMES: &[&str] = &[\"a\", \"b\"];\n",
		"pub const NAMES: &[&str] = &[\"a\"];\n",
	);

	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert!(change.assessment.is_none());
}

#[test]
fn slice_append_ignores_a_changed_declared_element_type() {
	// `Vec<T>` uses the same literal shape, so the declared element type is the
	// only signal that the collection contract changed.
	let change = diff_one_constant(
		"pub const NAMES: &[&str] = &[\"a\", \"b\"];\n",
		"pub const NAMES: Vec<String> = vec![\"a\", \"b\", \"c\"];\n",
	);

	assert_eq!(change.kind, SemanticChangeKind::Modified);
	assert!(change.assessment.is_none());
	assert_eq!(
		monochange_semver::semantic_change_severity(&change),
		BumpSeverity::Major
	);
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

#[test]
fn slice_initializer_requires_a_slice_shaped_declared_type() {
	let cases = [
		// A reference to something that is not a slice.
		(
			"pub const NAMES: &str = \"a\";\n",
			"pub const NAMES: &str = \"b\";\n",
		),
		// A plain scalar path type.
		(
			"pub const NAMES: String = String::new();\n",
			"pub const NAMES: String = String::from(\"a\");\n",
		),
		// A generic path type that is not `Vec`.
		(
			"pub const NAMES: Option<u8> = None;\n",
			"pub const NAMES: Option<u8> = Some(1);\n",
		),
		// A `Vec` whose only argument is a lifetime, not an element type.
		(
			"pub const NAMES: Vec<'a> = Vec::new();\n",
			"pub const NAMES: Vec<'a> = Vec::new();\n",
		),
		// A tuple type, which is neither a slice nor a `Vec`.
		(
			"pub const NAMES: (u8, u8) = (1, 2);\n",
			"pub const NAMES: (u8, u8) = (1, 3);\n",
		),
	];

	for (before_source, after_source) in cases {
		if before_source == after_source {
			// Identical sources produce no change to inspect.
			assert!(
				diff_public_symbols_allow_empty(before_source, after_source).is_empty(),
				"`{before_source}` should produce no symbol change"
			);
			continue;
		}

		let change = diff_one_constant(before_source, after_source);

		assert!(
			change.assessment.is_none(),
			"`{before_source}` is not a slice literal: {change:?}"
		);
		assert_eq!(
			monochange_semver::semantic_change_severity(&change),
			BumpSeverity::Major
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

#[test]
fn slice_initializer_ignores_a_vec_type_without_angle_arguments() {
	let change = diff_one_constant(
		"pub const NAMES: Vec = Vec::new();\n",
		"pub const NAMES: Vec = Vec::with_capacity(4);\n",
	);

	assert!(change.assessment.is_none());
	assert_eq!(
		monochange_semver::semantic_change_severity(&change),
		BumpSeverity::Major
	);
}

#[test]
fn slice_initializer_ignores_a_repeat_macro() {
	// `vec![value; count]` has no element sequence, so the parse fails and the
	// diff keeps the conservative verdict.
	let change = diff_one_constant(
		"pub static NAMES: Vec<u8> = vec![0; 2];\n",
		"pub static NAMES: Vec<u8> = vec![0; 3];\n",
	);

	assert!(change.assessment.is_none());
	assert_eq!(
		monochange_semver::semantic_change_severity(&change),
		BumpSeverity::Major
	);
}

#[test]
fn slice_initializer_ignores_a_non_vec_macro() {
	let change = diff_one_constant(
		"pub static NAMES: Vec<u8> = make_names();\n",
		"pub static NAMES: Vec<u8> = make_more_names();\n",
	);

	assert!(change.assessment.is_none());
	assert_eq!(
		monochange_semver::semantic_change_severity(&change),
		BumpSeverity::Major
	);
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
fn snapshot_public_symbols_collects_snapshot_parse_warnings() {
	let snapshot = PackageSnapshot {
		label: "HEAD".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("src/lib.rs"),
			contents: "pub fn broken(".to_string(),
		}],
	};

	let (_, warnings) = snapshot_public_symbols(Some(&snapshot), &[], DetectionLevel::Signature);

	assert_eq!(warnings.len(), 1);
	assert!(
		warnings
			.first()
			.unwrap_or_else(|| panic!("expected one parse warning"))
			.contains("failed to parse src/lib.rs")
	);
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
fn module_prefix_diff_and_manifest_helpers_cover_remaining_branches() {
	assert!(module_prefix_for_file(Path::new("src")).is_empty());

	let before = BTreeMap::from([
		(
			("function".to_string(), "greet".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "greet".to_string(),
				signature: "pub fn greet()".to_string(),
				initializer: None,
				file_path: PathBuf::from("src/lib.rs"),
			},
		),
		(
			("struct".to_string(), "Greeter".to_string()),
			PublicSymbol {
				item_kind: "struct".to_string(),
				item_path: "Greeter".to_string(),
				signature: "pub struct Greeter;".to_string(),
				initializer: None,
				file_path: PathBuf::from("src/lib.rs"),
			},
		),
	]);
	let after = BTreeMap::from([(
		("function".to_string(), "greet".to_string()),
		PublicSymbol {
			item_kind: "function".to_string(),
			item_path: "greet".to_string(),
			signature: "pub fn greet()".to_string(),
			initializer: None,
			file_path: PathBuf::from("src/lib.rs"),
		},
	)]);

	let changes = diff_public_symbols(&before, &after);

	assert_eq!(changes.len(), 1);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one removed change"));
	assert_eq!(change.kind, SemanticChangeKind::Removed);
	assert!(change.summary.contains("removed"));
	assert_eq!(describe_manifest_value(&Value::Boolean(true)), "true");
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
