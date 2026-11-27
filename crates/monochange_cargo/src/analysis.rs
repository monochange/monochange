use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::AnalyzedFileChange;
use monochange_core::ApiConfidence;
use monochange_core::ApiItem;
use monochange_core::ApiSnapshot;
use monochange_core::BumpSeverity;
use monochange_core::CargoSemverChecksSettings;
use monochange_core::DetectionLevel;
use monochange_core::Ecosystem;
use monochange_core::MonochangeResult;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageAnalysisResult;
use monochange_core::PackageRecord;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::SemanticAnalysisCompleteness;
use monochange_core::SemanticAnalysisOutcome;
use monochange_core::SemanticAnalyzer;
use monochange_core::SemanticAnalyzerEvidence;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeAssessment;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use quote::ToTokens;
use syn::parse::Parser as _;
use syn::visit_mut::VisitMut as _;
use toml::Value;

use crate::CARGO_MANIFEST_FILE;
use crate::semver_checks::CargoSemverAnalysis;
use crate::semver_checks::CargoSemverChecksAnalyzer;

/// Cargo analyzer that extracts public Rust API, dependency, and manifest metadata diffs.
#[derive(Debug)]
pub struct CargoSemanticAnalyzer {
	semver_checks: CargoSemverChecksAnalyzer,
}

impl Default for CargoSemanticAnalyzer {
	fn default() -> Self {
		Self {
			semver_checks: CargoSemverChecksAnalyzer::new(CargoSemverChecksSettings::default()),
		}
	}
}

/// Return the shared Cargo semantic analyzer.
#[must_use]
pub fn semantic_analyzer() -> CargoSemanticAnalyzer {
	CargoSemanticAnalyzer::default()
}

/// Return a Cargo semantic analyzer with an explicit compatibility-matrix configuration.
#[must_use]
pub fn semantic_analyzer_with_settings(
	settings: CargoSemverChecksSettings,
) -> CargoSemanticAnalyzer {
	CargoSemanticAnalyzer {
		semver_checks: CargoSemverChecksAnalyzer::new(settings),
	}
}

impl SemanticAnalyzer for CargoSemanticAnalyzer {
	fn analyzer_id(&self) -> &'static str {
		"cargo/public-api"
	}

	fn applies_to(&self, package: &PackageRecord) -> bool {
		package.ecosystem == Ecosystem::Cargo
	}

	fn analyze_package(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> MonochangeResult<PackageAnalysisResult> {
		let mut warnings = Vec::new();
		let mut semantic_changes = Vec::new();

		let (before_symbols, mut before_warnings) = snapshot_public_symbols(
			context.before_snapshot,
			context.changed_files,
			context.detection_level,
		);
		let (after_symbols, mut after_warnings) = snapshot_public_symbols(
			context.after_snapshot,
			context.changed_files,
			context.detection_level,
		);
		warnings.append(&mut before_warnings);
		warnings.append(&mut after_warnings);

		semantic_changes.extend(diff_public_symbols(&before_symbols, &after_symbols));
		merge_semver_analysis(&mut semantic_changes, self.semver_checks.analyze(context));

		if let Some(manifest_change) = context
			.changed_files
			.iter()
			.find(|change| change.package_path == Path::new(CARGO_MANIFEST_FILE))
		{
			semantic_changes.extend(analyze_manifest_change(manifest_change, &mut warnings));
		}

		semantic_changes.sort_by(|left, right| {
			(
				left.category,
				left.kind,
				left.item_kind.as_str(),
				left.item_path.as_str(),
			)
				.cmp(&(
					right.category,
					right.kind,
					right.item_kind.as_str(),
					right.item_path.as_str(),
				))
		});

		Ok(PackageAnalysisResult {
			analyzer_id: self.analyzer_id().to_string(),
			package_id: display_package_id(context.package),
			ecosystem: context.package.ecosystem,
			changed_files: context
				.changed_files
				.iter()
				.map(|file| file.package_path.clone())
				.collect(),
			semantic_changes,
			warnings,
		})
	}
}

fn merge_semver_analysis(
	semantic_changes: &mut Vec<SemanticChange>,
	analysis: Option<CargoSemverAnalysis>,
) {
	let Some(analysis) = analysis else {
		return;
	};

	if analysis.replace_syntax_changes {
		semantic_changes.retain(|change| {
			change.category != SemanticChangeCategory::PublicApi
				|| change.kind == SemanticChangeKind::Added
		});
	}

	semantic_changes.push(analysis.change);
}

fn display_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

/// Extract a monochange-owned API snapshot for a Cargo package.
pub fn api_snapshot(context: &PackageAnalysisContext<'_>) -> ApiSnapshot {
	let (symbols, warnings) = snapshot_public_symbols(
		context.after_snapshot.or(context.before_snapshot),
		context.changed_files,
		context.detection_level,
	);
	let items = symbols
		.into_values()
		.map(|symbol| {
			ApiItem::new(symbol.item_kind, symbol.item_path, Some(symbol.signature))
				.with_source_path(symbol.file_path)
		})
		.collect();

	ApiSnapshot::new(
		display_package_id(context.package),
		context.package.name.clone(),
		context.package.ecosystem,
		"cargo/public-api",
		items,
		warnings,
	)
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct PublicSymbol {
	item_kind: String,
	item_path: String,
	signature: String,
	/// Signature tokens excluding outer attributes on the item itself.
	signature_without_attrs: String,
	/// Retained (non-inert) outer attributes on the item itself.
	outer_attributes: Vec<RetainedAttribute>,
	/// Element sequence of a slice literal initializer, when the declared type
	/// is a growable slice. Used to recognize a pure append.
	initializer: Option<SliceInitializer>,
	/// Decomposed trait surface used to distinguish defaulted additions from
	/// breaking trait changes.
	trait_surface: Option<TraitSurface>,
	file_path: PathBuf,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct RetainedAttribute {
	rendered: String,
	category: AttributeCategory,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum AttributeCategory {
	Inline,
	MustUse,
	Deprecated,
	BreakingOrUnknown,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum TraitMemberKind {
	Const,
	Fn,
	Type,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct TraitMember {
	kind: TraitMemberKind,
	name: String,
	/// Signature tokens excluding outer attributes and default values/bodies.
	signature_without_attrs: String,
	/// Retained (non-inert) outer attributes on the trait member.
	outer_attributes: Vec<RetainedAttribute>,
	/// Whether this trait member supplies a default body, value, or type.
	has_default: bool,
	/// For defaulted associated constants and types, the default value/type tokens.
	/// Changing an existing default constant value or default associated type stays breaking.
	default_value: Option<String>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct TraitSurface {
	/// Trait declaration header without outer attributes (`vis`, `unsafety`,
	/// `modifiers`, `trait`, `ident`, `generics`, `supertraits`, `where_clause`).
	header_without_attrs: String,
	/// Set when the trait includes opaque macro/verbatim items or duplicate member
	/// keys (for example cfg-gated overloads) that syntax analysis cannot safely compare.
	has_opaque_items: bool,
	members: BTreeMap<(TraitMemberKind, String), TraitMember>,
}

struct SymbolData {
	signature: String,
	signature_without_attrs: String,
	outer_attributes: Vec<RetainedAttribute>,
	initializer: Option<SliceInitializer>,
	trait_surface: Option<TraitSurface>,
}

/// Element type and literal elements of a public `const`/`static` slice.
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct SliceInitializer {
	element_type: String,
	elements: Vec<String>,
}

impl SliceInitializer {
	/// Return whether `self` becomes `after` by appending elements in order.
	///
	/// A removal, a reorder, or an element edit changes an existing element
	/// position, so only a strict prefix relationship qualifies.
	fn appends_into(&self, after: &Self) -> bool {
		self.element_type == after.element_type
			&& after.elements.len() > self.elements.len()
			&& after.elements.starts_with(&self.elements)
	}
}

fn snapshot_public_symbols(
	snapshot: Option<&PackageSnapshot>,
	changed_files: &[AnalyzedFileChange],
	_detection_level: DetectionLevel,
) -> (BTreeMap<(String, String), PublicSymbol>, Vec<String>) {
	let mut warnings = Vec::new();
	let mut symbols = BTreeMap::new();

	if let Some(snapshot) = snapshot {
		for file in &snapshot.files {
			if !is_rust_source_file(file) {
				continue;
			}

			match collect_public_symbols(file) {
				Ok(file_symbols) => {
					for symbol in file_symbols {
						symbols
							.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
					}
				}
				Err(error) => warnings.push(error),
			}
		}

		return (symbols, warnings);
	}

	for change in changed_files {
		let contents = change
			.after_contents
			.as_deref()
			.or(change.before_contents.as_deref());
		let Some(contents) = contents else {
			continue;
		};
		let file = PackageSnapshotFile {
			path: change.package_path.clone(),
			contents: contents.to_string(),
		};

		if !is_rust_source_file(&file) {
			continue;
		}

		match collect_public_symbols(&file) {
			Ok(file_symbols) => {
				for symbol in file_symbols {
					symbols.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
				}
			}
			Err(error) => warnings.push(error),
		}
	}

	(symbols, warnings)
}

fn is_rust_source_file(file: &PackageSnapshotFile) -> bool {
	file.path.extension().and_then(|ext| ext.to_str()) == Some("rs") && file.path.starts_with("src")
}

fn collect_public_symbols(file: &PackageSnapshotFile) -> Result<Vec<PublicSymbol>, String> {
	let module_prefix = module_prefix_for_file(&file.path);
	let mut parsed = syn::parse_file(&file.contents)
		.map_err(|error| format!("failed to parse {}: {error}", file.path.display()))?;
	let mut symbols = Vec::new();
	RemoveInertAttributes.visit_file_mut(&mut parsed);
	let mut collector = SymbolCollector {
		module_prefix: &module_prefix,
		file_path: &file.path,
		output: &mut symbols,
	};
	collector.collect_items(&mut parsed.items);
	Ok(symbols)
}

/// Documentation prose, lint-level settings, and formatting directives do not
/// change Rust public API or ABI signatures.
struct RemoveInertAttributes;

impl syn::visit_mut::VisitMut for RemoveInertAttributes {
	fn visit_attributes_mut(&mut self, attributes: &mut Vec<syn::Attribute>) {
		attributes.retain_mut(|attribute| retain_api_attribute(&mut attribute.meta));
	}
}

/// Keep API-significant attributes while stripping inert docs, lint levels, and
/// formatting directives (including inside `cfg_attr`).
fn retain_api_attribute(meta: &mut syn::Meta) -> bool {
	if is_inert_lint_attribute(meta.path()) || is_rustfmt_skip_attribute(meta.path()) {
		return false;
	}

	if meta.path().is_ident("doc") {
		return retain_doc_attribute(meta);
	}

	let syn::Meta::List(list) = meta else {
		return true;
	};

	if !list.path.is_ident("cfg_attr") {
		return true;
	}

	let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
	let Ok(arguments) = parser.parse2(list.tokens.clone()) else {
		// Preserve unfamiliar attribute syntax rather than hiding API changes.
		return true;
	};

	let mut arguments = arguments.into_iter();
	let Some(condition) = arguments.next() else {
		return true;
	};
	let Some(first_attribute) = arguments.next() else {
		return true;
	};
	let attributes = std::iter::once(first_attribute)
		.chain(arguments)
		.filter_map(|mut attribute| retain_api_attribute(&mut attribute).then_some(attribute))
		.collect::<Vec<_>>();

	if attributes.is_empty() {
		return false;
	}

	list.tokens = quote::quote!(#condition, #(#attributes),*);
	true
}

fn is_inert_lint_attribute(path: &syn::Path) -> bool {
	path.get_ident().is_some_and(|ident| {
		matches!(
			ident.to_string().as_str(),
			"allow" | "warn" | "deny" | "forbid" | "expect"
		)
	})
}

fn is_rustfmt_skip_attribute(path: &syn::Path) -> bool {
	let mut segments = path.segments.iter();
	matches!(
		(segments.next(), segments.next(), segments.next()),
		(Some(first), Some(second), None) if first.ident == "rustfmt" && second.ident == "skip"
	)
}

/// Strip documentation prose and rustdoc presentation options while keeping
/// `#[doc(hidden)]`, which marks the documented `SemVer` public API boundary.
fn retain_doc_attribute(meta: &mut syn::Meta) -> bool {
	let syn::Meta::List(list) = meta else {
		return false;
	};

	let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
	let Ok(arguments) = parser.parse2(list.tokens.clone()) else {
		// Preserve unrecognized `#[doc(...)]` syntax conservatively.
		return true;
	};

	let retained = arguments
		.into_iter()
		.filter(|argument| matches!(argument, syn::Meta::Path(path) if path.is_ident("hidden")))
		.collect::<Vec<_>>();

	if retained.is_empty() {
		return false;
	}

	list.tokens = quote::quote!(#(#retained),*);
	true
}

fn extract_retained_attributes(attributes: &[syn::Attribute]) -> Vec<RetainedAttribute> {
	attributes
		.iter()
		.map(|attribute| {
			RetainedAttribute {
				rendered: render_signature(attribute),
				category: classify_meta_category(&attribute.meta),
			}
		})
		.collect()
}

fn classify_meta_category(meta: &syn::Meta) -> AttributeCategory {
	match meta.path().get_ident().map(ToString::to_string).as_deref() {
		Some("inline") => AttributeCategory::Inline,
		Some("must_use") => AttributeCategory::MustUse,
		Some("deprecated") => AttributeCategory::Deprecated,
		Some("cfg_attr") => classify_cfg_attr_category(meta),
		_ => AttributeCategory::BreakingOrUnknown,
	}
}

fn classify_cfg_attr_category(meta: &syn::Meta) -> AttributeCategory {
	let syn::Meta::List(list) = meta else {
		return AttributeCategory::BreakingOrUnknown;
	};

	let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
	let Ok(arguments) = parser.parse2(list.tokens.clone()) else {
		return AttributeCategory::BreakingOrUnknown;
	};

	let mut inner_categories = arguments
		.into_iter()
		.skip(1)
		.map(|argument| classify_meta_category(&argument));

	match inner_categories.next() {
		Some(first) => inner_categories.fold(first, std::cmp::max),
		None => AttributeCategory::BreakingOrUnknown,
	}
}

fn has_macro_export(attributes: &[syn::Attribute]) -> bool {
	attributes
		.iter()
		.any(|attribute| meta_contains_macro_export(&attribute.meta))
}

fn meta_contains_macro_export(meta: &syn::Meta) -> bool {
	if meta.path().is_ident("macro_export") {
		return true;
	}

	let syn::Meta::List(list) = meta else {
		return false;
	};

	if !list.path.is_ident("cfg_attr") {
		return false;
	}

	let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
	let Ok(arguments) = parser.parse2(list.tokens.clone()) else {
		return false;
	};

	arguments
		.into_iter()
		.skip(1)
		.any(|argument| meta_contains_macro_export(&argument))
}

fn render_item_signatures<T: ToTokens>(
	item: &mut T,
	attrs_mut: impl Fn(&mut T) -> &mut Vec<syn::Attribute>,
) -> (String, String, Vec<RetainedAttribute>) {
	let signature = render_signature(item);
	let outer_attributes = extract_retained_attributes(attrs_mut(item));
	let saved = std::mem::take(attrs_mut(item));
	let signature_without_attrs = render_signature(item);
	*attrs_mut(item) = saved;
	(signature, signature_without_attrs, outer_attributes)
}

fn render_signature_with_attrs(attrs: &[syn::Attribute], value: &impl ToTokens) -> String {
	if attrs.is_empty() {
		render_signature(value)
	} else {
		quote::quote!(#(#attrs)* #value).to_string()
	}
}

fn render_attributes(attrs: &[syn::Attribute]) -> String {
	quote::quote!(#(#attrs)*).to_string()
}

fn build_trait_surface(item: &mut syn::ItemTrait) -> TraitSurface {
	let mut has_opaque_items = false;
	let mut members = BTreeMap::new();

	for trait_item in &mut item.items {
		let Some(member) = extract_trait_member(trait_item) else {
			has_opaque_items = true;
			continue;
		};
		let key = (member.kind, member.name.clone());
		if members.insert(key, member).is_some() {
			has_opaque_items = true;
		}
	}

	let saved_attrs = std::mem::take(&mut item.attrs);
	let saved_items = std::mem::take(&mut item.items);
	let header_without_attrs = render_signature(item);
	item.attrs = saved_attrs;
	item.items = saved_items;

	TraitSurface {
		header_without_attrs,
		has_opaque_items,
		members,
	}
}

fn extract_trait_member(item: &mut syn::TraitItem) -> Option<TraitMember> {
	match item {
		syn::TraitItem::Fn(method) => {
			Some(TraitMember {
				kind: TraitMemberKind::Fn,
				name: method.sig.ident.to_string(),
				signature_without_attrs: render_signature(&method.sig),
				outer_attributes: extract_retained_attributes(&method.attrs),
				has_default: method.default.is_some(),
				default_value: None,
			})
		}

		syn::TraitItem::Const(constant) => {
			let name = constant.ident.to_string();
			let outer_attributes = extract_retained_attributes(&constant.attrs);
			let saved_attrs = std::mem::take(&mut constant.attrs);
			let saved_default = constant.default.take();
			let has_default = saved_default.is_some();
			let default_value = saved_default
				.as_ref()
				.map(|(_, expr)| render_signature(expr));
			let signature_without_attrs = render_signature(constant);
			constant.attrs = saved_attrs;
			constant.default = saved_default;

			Some(TraitMember {
				kind: TraitMemberKind::Const,
				name,
				signature_without_attrs,
				outer_attributes,
				has_default,
				default_value,
			})
		}

		syn::TraitItem::Type(assoc_type) => {
			let name = assoc_type.ident.to_string();
			let outer_attributes = extract_retained_attributes(&assoc_type.attrs);
			let saved_attrs = std::mem::take(&mut assoc_type.attrs);
			let saved_default = assoc_type.default.take();
			let has_default = saved_default.is_some();
			let default_value = saved_default.as_ref().map(|(_, ty)| render_signature(ty));
			let signature_without_attrs = render_signature(assoc_type);
			assoc_type.attrs = saved_attrs;
			assoc_type.default = saved_default;

			Some(TraitMember {
				kind: TraitMemberKind::Type,
				name,
				signature_without_attrs,
				outer_attributes,
				has_default,
				default_value,
			})
		}

		_ => None,
	}
}

struct SymbolCollector<'a> {
	module_prefix: &'a [String],
	file_path: &'a Path,
	output: &'a mut Vec<PublicSymbol>,
}

impl SymbolCollector<'_> {
	fn collect_items(&mut self, items: &mut [syn::Item]) {
		for item in items {
			self.collect_item(item);
		}
	}

	fn push_item<T: ToTokens>(
		&mut self,
		item_kind: &str,
		item_name: String,
		item: &mut T,
		attrs_mut: impl Fn(&mut T) -> &mut Vec<syn::Attribute>,
	) {
		self.push_item_with_extras(item_kind, item_name, item, attrs_mut, (None, None));
	}

	fn push_item_with_extras<T: ToTokens>(
		&mut self,
		item_kind: &str,
		item_name: String,
		item: &mut T,
		attrs_mut: impl Fn(&mut T) -> &mut Vec<syn::Attribute>,
		extras: (Option<SliceInitializer>, Option<TraitSurface>),
	) {
		let (signature, signature_without_attrs, outer_attributes) =
			render_item_signatures(item, attrs_mut);

		self.push_symbol(
			item_kind,
			item_name,
			SymbolData {
				signature,
				signature_without_attrs,
				outer_attributes,
				initializer: extras.0,
				trait_surface: extras.1,
			},
		);
	}

	fn collect_item(&mut self, item: &mut syn::Item) {
		match item {
			syn::Item::Const(item) if is_public(&item.vis) => {
				let initializer = slice_initializer(&item.ty, &item.expr);
				let name = item.ident.to_string();
				self.push_item_with_extras(
					"constant",
					name,
					item,
					|item| &mut item.attrs,
					(initializer, None),
				);
			}

			syn::Item::Enum(item) if is_public(&item.vis) => {
				let name = item.ident.to_string();
				self.push_item("enum", name, item, |item| &mut item.attrs);
			}

			syn::Item::Fn(item) if is_public(&item.vis) => {
				let signature_without_attrs = render_signature(&item.sig);
				let signature = render_signature_with_attrs(&item.attrs, &item.sig);
				let outer_attributes = extract_retained_attributes(&item.attrs);
				self.push_symbol(
					"function",
					item.sig.ident.to_string(),
					SymbolData {
						signature,
						signature_without_attrs,
						outer_attributes,
						initializer: None,
						trait_surface: None,
					},
				);
			}

			syn::Item::Mod(item) if is_public(&item.vis) => {
				self.collect_module(item);
			}

			syn::Item::Static(item) if is_public(&item.vis) => {
				let initializer = slice_initializer(&item.ty, &item.expr);
				let name = item.ident.to_string();
				self.push_item_with_extras(
					"static",
					name,
					item,
					|item| &mut item.attrs,
					(initializer, None),
				);
			}

			syn::Item::Struct(item) if is_public(&item.vis) => {
				let name = item.ident.to_string();
				self.push_item("struct", name, item, |item| &mut item.attrs);
			}

			syn::Item::Trait(item) if is_public(&item.vis) => {
				let trait_surface = Some(build_trait_surface(item));
				let name = item.ident.to_string();
				self.push_item_with_extras(
					"trait",
					name,
					item,
					|item| &mut item.attrs,
					(None, trait_surface),
				);
			}

			syn::Item::Type(item) if is_public(&item.vis) => {
				let name = item.ident.to_string();
				self.push_item("type_alias", name, item, |item| &mut item.attrs);
			}

			syn::Item::Union(item) if is_public(&item.vis) => {
				let name = item.ident.to_string();
				self.push_item("union", name, item, |item| &mut item.attrs);
			}

			syn::Item::Use(item) if is_public(&item.vis) => {
				self.collect_reexport(item);
			}

			syn::Item::Macro(item)
				if item.ident.is_some()
					&& item.mac.path.is_ident("macro_rules")
					&& has_macro_export(&item.attrs) =>
			{
				self.collect_exported_macro(item);
			}

			_ => {}
		}
	}

	fn collect_module(&mut self, item: &mut syn::ItemMod) {
		let module_name = item.ident.to_string();
		self.push_item("module", module_name.clone(), item, |item| &mut item.attrs);

		if let Some((_, nested_items)) = &mut item.content {
			let mut nested_prefix = self.module_prefix.to_vec();
			nested_prefix.push(module_name);
			let mut nested = SymbolCollector {
				module_prefix: &nested_prefix,
				file_path: self.file_path,
				output: self.output,
			};
			nested.collect_items(nested_items);
		}
	}

	fn collect_reexport(&mut self, item: &syn::ItemUse) {
		let use_tree = render_signature(&item.tree);
		let signature_without_attrs = format!("pub use {use_tree};");
		let signature = if item.attrs.is_empty() {
			signature_without_attrs.clone()
		} else {
			format!(
				"{} {signature_without_attrs}",
				render_attributes(&item.attrs)
			)
		};

		let outer_attributes = extract_retained_attributes(&item.attrs);
		self.push_symbol(
			"reexport",
			use_tree,
			SymbolData {
				signature,
				signature_without_attrs,
				outer_attributes,
				initializer: None,
				trait_surface: None,
			},
		);
	}

	fn collect_exported_macro(&mut self, item: &mut syn::ItemMacro) {
		let Some(ident) = item.ident.as_ref().map(ToString::to_string) else {
			return;
		};

		let mut root_collector = SymbolCollector {
			module_prefix: &[],
			file_path: self.file_path,
			output: self.output,
		};
		root_collector.push_item("macro", ident, item, |item| &mut item.attrs);
	}

	fn push_symbol(&mut self, item_kind: &str, item_name: String, data: SymbolData) {
		let item_path = if self.module_prefix.is_empty() {
			item_name
		} else {
			format!("{}::{item_name}", self.module_prefix.join("::"))
		};

		self.output.push(PublicSymbol {
			item_kind: item_kind.to_string(),
			item_path,
			signature: data.signature,
			signature_without_attrs: data.signature_without_attrs,
			outer_attributes: data.outer_attributes,
			initializer: data.initializer,
			trait_surface: data.trait_surface,
			file_path: self.file_path.to_path_buf(),
		});
	}
}

fn is_public(visibility: &syn::Visibility) -> bool {
	matches!(visibility, syn::Visibility::Public(_))
}

fn render_signature(value: &impl ToTokens) -> String {
	value.to_token_stream().to_string()
}

/// Extract the element sequence of a public slice literal.
///
/// Only `const`/`static` items whose declared type is a slice, `Vec`, or
/// fixed-size array and whose initializer is a literal produce a sequence.
/// Everything else returns `None`, which keeps the diff conservative.
fn slice_initializer(
	declared_type: &syn::Type,
	expression: &syn::Expr,
) -> Option<SliceInitializer> {
	let element_type = slice_element_type(declared_type)?;

	Some(SliceInitializer {
		element_type,
		elements: literal_elements(expression)?,
	})
}

/// Return the literal elements of a slice or `vec![]` initializer.
fn literal_elements(expression: &syn::Expr) -> Option<Vec<String>> {
	match expression {
		// `&[..]` is how a `&[T]` constant is written.
		syn::Expr::Reference(reference) => literal_elements(&reference.expr),
		syn::Expr::Array(array) => Some(array.elems.iter().map(render_signature).collect()),

		syn::Expr::Macro(expression) if expression.mac.path.is_ident("vec") => {
			// The repeat form `vec![value; count]` is not an element sequence,
			// so a failed parse falls through to the conservative default.
			expression
				.mac
				.parse_body_with(
					syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
				)
				.ok()
				.map(|elements| elements.iter().map(render_signature).collect())
		}

		_ => None,
	}
}

/// Return the element type of a slice-shaped declared type.
fn slice_element_type(declared_type: &syn::Type) -> Option<String> {
	let declared_type = match declared_type {
		// `&[T]` is how a slice constant is written; the reference itself is not
		// part of the element type.
		syn::Type::Reference(reference) => reference.elem.as_ref(),
		other => other,
	};

	match declared_type {
		syn::Type::Slice(slice) => Some(render_signature(&slice.elem)),
		syn::Type::Array(array) => Some(render_signature(&array.elem)),
		syn::Type::Path(path) => vec_element_type(path),
		// Includes every other declared type such as a tuple or a pointer.
		_ => None,
	}
}

/// Return the element type of a `Vec<T>` path.
fn vec_element_type(path: &syn::TypePath) -> Option<String> {
	let segment = path.path.segments.last()?;

	if segment.ident != "Vec" {
		return None;
	}

	let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
		return None;
	};
	arguments.args.iter().find_map(|argument| {
		match argument {
			syn::GenericArgument::Type(inner) => Some(render_signature(inner)),
			// A lifetime or const argument is not an element type.
			_ => None,
		}
	})
}

fn module_prefix_for_file(path: &Path) -> Vec<String> {
	let mut components = path
		.components()
		.map(|component| component.as_os_str().to_string_lossy().to_string())
		.collect::<Vec<_>>();

	if components
		.first()
		.is_some_and(|component| component == "src")
	{
		components.remove(0);
	}

	let Some(last) = components.pop() else {
		return Vec::new();
	};

	let stem = last.strip_suffix(".rs").unwrap_or(&last);

	if stem != "lib" && stem != "main" && stem != "mod" {
		components.push(stem.to_string());
	}

	components
}

fn diff_public_symbols(
	before: &BTreeMap<(String, String), PublicSymbol>,
	after: &BTreeMap<(String, String), PublicSymbol>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	for (key, after_symbol) in after {
		match before.get(key) {
			None => {
				changes.push(build_symbol_change(
					SemanticChangeKind::Added,
					after_symbol,
					None,
					Some(after_symbol.signature.clone()),
				));
			}

			Some(before_symbol) if before_symbol.signature != after_symbol.signature => {
				let mut change = build_symbol_change(
					SemanticChangeKind::Modified,
					after_symbol,
					Some(before_symbol.signature.clone()),
					Some(after_symbol.signature.clone()),
				);
				if let Some(assessment) = modified_symbol_assessment(before_symbol, after_symbol) {
					change.assessment = Some(assessment);
				}

				changes.push(change);
			}

			Some(_) => {}
		}
	}

	for (key, before_symbol) in before {
		if after.contains_key(key) {
			continue;
		}

		changes.push(build_symbol_change(
			SemanticChangeKind::Removed,
			before_symbol,
			Some(before_symbol.signature.clone()),
			None,
		));
	}

	changes
}

fn build_symbol_change(
	kind: SemanticChangeKind,
	symbol: &PublicSymbol,
	before_signature: Option<String>,
	after_signature: Option<String>,
) -> SemanticChange {
	let verb = if kind == SemanticChangeKind::Added {
		"added"
	} else if kind == SemanticChangeKind::Removed {
		"removed"
	} else {
		"modified"
	};

	let mut change = SemanticChange::new(
		SemanticChangeCategory::PublicApi,
		kind,
		symbol.item_kind.clone(),
		symbol.item_path.clone(),
		format!("{} `{}` {verb}", symbol.item_kind, symbol.item_path),
		symbol.file_path.clone(),
	);
	change.before_signature = before_signature;
	change.after_signature = after_signature;
	change
}

fn modified_symbol_assessment(
	before_symbol: &PublicSymbol,
	after_symbol: &PublicSymbol,
) -> Option<SemanticChangeAssessment> {
	if let Some(assessment) = slice_append_assessment(before_symbol, after_symbol) {
		return Some(assessment);
	}

	if let Some(assessment) = trait_change_assessment(before_symbol, after_symbol) {
		return Some(assessment);
	}

	attribute_only_assessment(before_symbol, after_symbol)
}

/// Assess a slice literal that only gained elements as additive.
///
/// The whole-item signature cannot distinguish an append from a removal or an
/// element edit, so this inspects the parsed element sequence instead. A
/// removal, a reorder, an element edit, a changed element type, or a breaking
/// attribute change keeps the conservative `major` mapping that
/// `monochange_semver` derives from `(PublicApi, Modified)`.
fn slice_append_assessment(
	before_symbol: &PublicSymbol,
	after_symbol: &PublicSymbol,
) -> Option<SemanticChangeAssessment> {
	let before = before_symbol.initializer.as_ref()?;
	let after = after_symbol.initializer.as_ref()?;

	if !before.appends_into(after) {
		return None;
	}

	if before_symbol.outer_attributes != after_symbol.outer_attributes
		&& compatible_attribute_delta_bump(
			&before_symbol.outer_attributes,
			&after_symbol.outer_attributes,
		)
		.is_none()
	{
		return None;
	}

	Some(SemanticChangeAssessment::new(
		SemanticAnalysisOutcome::Additive,
		BumpSeverity::Minor,
		ApiConfidence::High,
		SemanticAnalyzerEvidence::new(
			"cargo/public-api",
			"monochange",
			SemanticAnalysisCompleteness::Complete,
			format!(
				"{} `{}` appends elements to a public `{}` slice literal; the element type and the existing element order are unchanged",
				after_symbol.item_kind, after_symbol.item_path, before.element_type
			),
		),
	))
}

/// Assess trait modifications where the header and every existing member
/// contract remain intact and any new member supplies a default.
fn trait_change_assessment(
	before_symbol: &PublicSymbol,
	after_symbol: &PublicSymbol,
) -> Option<SemanticChangeAssessment> {
	let before = before_symbol.trait_surface.as_ref()?;
	let after = after_symbol.trait_surface.as_ref()?;

	if before.has_opaque_items
		|| after.has_opaque_items
		|| before.header_without_attrs != after.header_without_attrs
	{
		return None;
	}

	let mut bump = trait_outer_attribute_bump(before_symbol, after_symbol)?;
	let (existing_gained_default, existing_attr_bump) =
		assess_existing_trait_members(before, after)?;
	bump = bump.max(existing_attr_bump);
	let added_defaulted = assess_added_trait_members(before, after)?;
	let added_or_defaulted = existing_gained_default || added_defaulted;

	let (outcome, suggested_bump, coverage) = if added_or_defaulted {
		(
			SemanticAnalysisOutcome::Additive,
			bump.max(BumpSeverity::Minor),
			format!(
				"trait `{}` only added defaulted items or defaults for existing items; downstream method-name collisions and dyn-compatibility are not checked by syntax analysis",
				after_symbol.item_path
			),
		)
	} else {
		(
			SemanticAnalysisOutcome::Compatible,
			bump.max(BumpSeverity::Patch),
			format!(
				"trait `{}` kept its header and required/defaulted item signatures unchanged; behavioral and downstream compatibility are not checked by syntax analysis",
				after_symbol.item_path
			),
		)
	};

	Some(SemanticChangeAssessment::new(
		outcome,
		suggested_bump,
		ApiConfidence::Medium,
		SemanticAnalyzerEvidence::new(
			"cargo/public-api",
			"monochange",
			SemanticAnalysisCompleteness::Partial,
			coverage,
		),
	))
}

fn trait_outer_attribute_bump(
	before_symbol: &PublicSymbol,
	after_symbol: &PublicSymbol,
) -> Option<BumpSeverity> {
	if before_symbol.outer_attributes == after_symbol.outer_attributes {
		return Some(BumpSeverity::None);
	}

	compatible_attribute_delta_bump(
		&before_symbol.outer_attributes,
		&after_symbol.outer_attributes,
	)
}

fn assess_existing_trait_members(
	before: &TraitSurface,
	after: &TraitSurface,
) -> Option<(bool, BumpSeverity)> {
	let mut gained_default = false;
	let mut bump = BumpSeverity::None;

	for (key, before_member) in &before.members {
		let after_member = after.members.get(key)?;
		let (member_gained_default, member_bump) =
			assess_existing_trait_member(before_member, after_member)?;
		gained_default |= member_gained_default;
		bump = bump.max(member_bump);
	}

	Some((gained_default, bump))
}

fn assess_existing_trait_member(
	before_member: &TraitMember,
	after_member: &TraitMember,
) -> Option<(bool, BumpSeverity)> {
	let default_value_changed = before_member.has_default
		&& after_member.has_default
		&& before_member.default_value != after_member.default_value;

	if before_member.signature_without_attrs != after_member.signature_without_attrs
		|| (before_member.has_default && !after_member.has_default)
		|| default_value_changed
	{
		return None;
	}

	let attr_bump = if before_member.outer_attributes == after_member.outer_attributes {
		BumpSeverity::None
	} else {
		compatible_attribute_delta_bump(
			&before_member.outer_attributes,
			&after_member.outer_attributes,
		)?
	};

	let gained_default = !before_member.has_default && after_member.has_default;
	Some((gained_default, attr_bump))
}

fn assess_added_trait_members(before: &TraitSurface, after: &TraitSurface) -> Option<bool> {
	let mut added_defaulted = false;

	for (key, after_member) in &after.members {
		if before.members.contains_key(key) {
			continue;
		}

		if !after_member.has_default {
			return None;
		}

		added_defaulted = true;
	}

	Some(added_defaulted)
}

/// Assess an item modification where only compatible outer attributes changed
/// (`#[inline]`, `#[must_use]`, or `#[deprecated]`, including inside `cfg_attr`).
fn attribute_only_assessment(
	before_symbol: &PublicSymbol,
	after_symbol: &PublicSymbol,
) -> Option<SemanticChangeAssessment> {
	if before_symbol.signature_without_attrs != after_symbol.signature_without_attrs {
		return None;
	}

	let suggested_bump = compatible_attribute_delta_bump(
		&before_symbol.outer_attributes,
		&after_symbol.outer_attributes,
	)?;

	Some(SemanticChangeAssessment::new(
		SemanticAnalysisOutcome::Compatible,
		suggested_bump,
		ApiConfidence::Medium,
		SemanticAnalyzerEvidence::new(
			"cargo/public-api",
			"monochange",
			SemanticAnalysisCompleteness::Partial,
			format!(
				"{} `{}` changed only compatible outer attributes (`#[inline]`, `#[must_use]`, or `#[deprecated]`); downstream lint sensitivity and macro expansion are not checked by syntax analysis",
				after_symbol.item_kind, after_symbol.item_path
			),
		),
	))
}

/// Return the compatible bump when two attribute lists differ only in `inline`,
/// `must_use`, or `deprecated` attributes, or `None` when any ABI-, visibility-,
/// cfg-, derive-, or unknown/proc-macro attribute changed.
fn compatible_attribute_delta_bump(
	before: &[RetainedAttribute],
	after: &[RetainedAttribute],
) -> Option<BumpSeverity> {
	let before_counts = count_retained_attributes(before);
	let after_counts = count_retained_attributes(after);
	let (any_added, added_deprecated) = classify_added_attributes(&before_counts, &after_counts)?;
	let any_removed = classify_removed_attributes(&before_counts, &after_counts)?;

	if !any_added && !any_removed {
		// Only attribute order changed; stay conservative when any breaking or
		// proc-macro attribute is present because expansion order can matter.
		let has_breaking = after
			.iter()
			.any(|attr| attr.category == AttributeCategory::BreakingOrUnknown);

		return (!has_breaking).then_some(BumpSeverity::Patch);
	}

	let bump = if added_deprecated {
		BumpSeverity::Minor
	} else {
		BumpSeverity::Patch
	};

	Some(bump)
}

fn count_retained_attributes(
	attributes: &[RetainedAttribute],
) -> BTreeMap<&RetainedAttribute, usize> {
	let mut counts = BTreeMap::new();

	for attribute in attributes {
		*counts.entry(attribute).or_insert(0) += 1;
	}

	counts
}

fn classify_added_attributes(
	before_counts: &BTreeMap<&RetainedAttribute, usize>,
	after_counts: &BTreeMap<&RetainedAttribute, usize>,
) -> Option<(bool, bool)> {
	let mut any_added = false;
	let mut added_deprecated = false;

	for (attr, after_count) in after_counts {
		let before_count = before_counts.get(attr).copied().unwrap_or(0);

		if *after_count <= before_count {
			continue;
		}

		any_added = true;

		match attr.category {
			AttributeCategory::BreakingOrUnknown => return None,
			AttributeCategory::Deprecated => added_deprecated = true,
			AttributeCategory::Inline | AttributeCategory::MustUse => {}
		}
	}

	Some((any_added, added_deprecated))
}

fn classify_removed_attributes(
	before_counts: &BTreeMap<&RetainedAttribute, usize>,
	after_counts: &BTreeMap<&RetainedAttribute, usize>,
) -> Option<bool> {
	let mut any_removed = false;

	for (attr, before_count) in before_counts {
		let after_count = after_counts.get(attr).copied().unwrap_or(0);

		if *before_count <= after_count {
			continue;
		}

		any_removed = true;

		if attr.category == AttributeCategory::BreakingOrUnknown {
			return None;
		}
	}

	Some(any_removed)
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ManifestEntry {
	item_kind: String,
	value: String,
}

fn analyze_manifest_change(
	change: &AnalyzedFileChange,
	warnings: &mut Vec<String>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	let before_manifest = parse_manifest(
		change.before_contents.as_deref(),
		&change.package_path,
		warnings,
	);
	let after_manifest = parse_manifest(
		change.after_contents.as_deref(),
		&change.package_path,
		warnings,
	);

	let before_dependencies = before_manifest
		.as_ref()
		.map(extract_dependency_entries)
		.unwrap_or_default();
	let after_dependencies = after_manifest
		.as_ref()
		.map(extract_dependency_entries)
		.unwrap_or_default();
	changes.extend(compare_manifest_entries(
		SemanticChangeCategory::Dependency,
		&change.package_path,
		&before_dependencies,
		&after_dependencies,
	));

	let before_metadata = before_manifest
		.as_ref()
		.map(extract_metadata_entries)
		.unwrap_or_default();
	let after_metadata = after_manifest
		.as_ref()
		.map(extract_metadata_entries)
		.unwrap_or_default();
	changes.extend(compare_manifest_entries(
		SemanticChangeCategory::Metadata,
		&change.package_path,
		&before_metadata,
		&after_metadata,
	));

	changes
}

fn parse_manifest(
	contents: Option<&str>,
	path: &Path,
	warnings: &mut Vec<String>,
) -> Option<Value> {
	let contents = contents?;

	match toml::from_str::<Value>(contents) {
		Ok(value) => Some(value),
		Err(error) => {
			warnings.push(format!("failed to parse {}: {error}", path.display()));
			None
		}
	}
}

fn extract_dependency_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
		let Some(table) = value.get(section).and_then(Value::as_table) else {
			continue;
		};

		for (name, dependency) in table {
			entries.insert(
				name.clone(),
				ManifestEntry {
					item_kind: "dependency".to_string(),
					value: format!("[{section}] {}", describe_manifest_value(dependency)),
				},
			);
		}
	}

	entries
}

fn extract_metadata_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for field in ["edition", "rust-version", "publish"] {
		if let Some(field_value) = value
			.get("package")
			.and_then(Value::as_table)
			.and_then(|table| table.get(field))
		{
			entries.insert(
				format!("package.{field}"),
				ManifestEntry {
					item_kind: "manifest_field".to_string(),
					value: describe_manifest_value(field_value),
				},
			);
		}
	}

	if let Some(features) = value.get("features").and_then(Value::as_table) {
		for (name, feature) in features {
			entries.insert(
				format!("feature.{name}"),
				ManifestEntry {
					item_kind: "feature".to_string(),
					value: describe_manifest_value(feature),
				},
			);
		}
	}

	entries
}

fn compare_manifest_entries(
	category: SemanticChangeCategory,
	file_path: &Path,
	before: &BTreeMap<String, ManifestEntry>,
	after: &BTreeMap<String, ManifestEntry>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	for (name, after_entry) in after {
		match before.get(name) {
			None => {
				changes.push(build_manifest_change(
					category,
					SemanticChangeKind::Added,
					file_path,
					name,
					after_entry,
					None,
					Some(after_entry.value.clone()),
				));
			}

			Some(before_entry) if before_entry != after_entry => {
				changes.push(build_manifest_change(
					category,
					SemanticChangeKind::Modified,
					file_path,
					name,
					after_entry,
					Some(before_entry.value.clone()),
					Some(after_entry.value.clone()),
				));
			}

			Some(_) => {}
		}
	}

	for (name, before_entry) in before {
		if after.contains_key(name) {
			continue;
		}

		changes.push(build_manifest_change(
			category,
			SemanticChangeKind::Removed,
			file_path,
			name,
			before_entry,
			Some(before_entry.value.clone()),
			None,
		));
	}

	changes
}

fn build_manifest_change(
	category: SemanticChangeCategory,
	kind: SemanticChangeKind,
	file_path: &Path,
	item_path: &str,
	entry: &ManifestEntry,
	before_signature: Option<String>,
	after_signature: Option<String>,
) -> SemanticChange {
	let verb = if kind == SemanticChangeKind::Added {
		"added"
	} else if kind == SemanticChangeKind::Removed {
		"removed"
	} else {
		"modified"
	};

	let mut change = SemanticChange::new(
		category,
		kind,
		entry.item_kind.clone(),
		item_path,
		format!("{} `{}` {verb}", entry.item_kind, item_path),
		file_path,
	);
	change.before_signature = before_signature;
	change.after_signature = after_signature;
	change
}

fn describe_manifest_value(value: &Value) -> String {
	match value {
		Value::String(text) => text.clone(),
		Value::Array(items) => {
			items
				.iter()
				.map(describe_manifest_value)
				.collect::<Vec<_>>()
				.join(", ")
		}
		Value::Table(table) => {
			table
				.iter()
				.map(|(key, value)| format!("{key}={}", describe_manifest_value(value)))
				.collect::<Vec<_>>()
				.join(", ")
		}
		other => other.to_string(),
	}
}

#[cfg(test)]
#[path = "__tests__/analysis_tests.rs"]
mod tests;
