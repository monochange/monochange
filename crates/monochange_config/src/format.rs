//! Canonical formatting for `monochange.toml`.
//!
//! Taplo formats TOML whitespace and can alphabetize keys inside a table, but
//! it does not reorder tables. This module owns the monochange-specific
//! ordering (sections, package tables, groups, and group members) and then
//! delegates the final whitespace pass to taplo.

use std::cmp::Ordering;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use toml_edit::Array;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::Table;
use toml_edit::Value;

/// Canonical order for top-level `monochange.toml` sections.
///
/// Tables not listed here keep their relative order after the known sections.
pub(crate) const ROOT_SECTION_ORDER: &[&str] = &[
	"defaults",
	"changelog",
	"version_scheme",
	"package",
	"group",
	"deployments",
	"lints",
	"ecosystems",
	"source",
	"prerelease",
	"changesets",
	"cli",
];

/// Reorder and format a `monochange.toml` document.
///
/// Comments and formatting attached to a table move with that table. The
/// document header (the comments before the first table) stays at the top.
pub fn format_workspace_configuration(source: &str) -> MonochangeResult<String> {
	if source.is_empty() {
		return Ok(String::new());
	}

	let (header, body) = split_document_header(source);
	let header = normalize_header(header);

	if body.trim().is_empty() {
		return Ok(header);
	}

	let mut document = body.parse::<DocumentMut>().map_err(|error| {
		MonochangeError::Config(format!("failed to parse monochange.toml: {error}"))
	})?;

	reorder_document(&mut document);

	let reordered = document.to_string();
	let formatted = taplo::formatter::format(&reordered, formatter_options());
	let formatted = if header.is_empty() {
		formatted.trim_start_matches(['\r', '\n']).to_string()
	} else {
		formatted
	};
	let mut output = String::with_capacity(header.len() + formatted.len());
	output.push_str(&header);
	output.push_str(&formatted);

	if !output.is_empty() && !output.ends_with('\n') {
		// patch-coverage:ignore-start -- taplo always emits a trailing newline; this is a defensive fallback.
		output.push('\n');
		// patch-coverage:ignore-end
	}

	Ok(output)
}

/// Split comments before the first table from the rest of the document.
///
/// Keeping this prefix out of the parsed document means a file header such as
/// `#:schema` and the repository title block stays at the top even when the
/// first table moves.
fn split_document_header(source: &str) -> (&str, &str) {
	let mut offset = 0;

	for line in source.split_inclusive('\n') {
		let trimmed = line.trim();

		if trimmed.is_empty() || trimmed.starts_with('#') {
			offset += line.len();
			continue;
		}

		if trimmed.starts_with('[') {
			return source.split_at(offset);
		}

		return ("", source);
	}

	(source, "")
}

/// Keep the header verbatim apart from ensuring one blank line before the
/// first table.
fn normalize_header(header: &str) -> String {
	let header = header.trim_end_matches(['\r', '\n']);

	if header.is_empty() {
		return String::new();
	}

	format!("{header}\n\n")
}

fn reorder_document(document: &mut DocumentMut) {
	let table = document.as_table_mut();
	reorder_table_keys(table, ROOT_SECTION_ORDER);
	let section_headers = [
		("package", sort_child_tables(table, "package")),
		("group", sort_child_tables(table, "group")),
		("version_scheme", sort_child_tables(table, "version_scheme")),
		("ecosystems", sort_child_tables(table, "ecosystems")),
		("cli", sort_child_tables(table, "cli")),
	];
	sort_group_packages(table);
	normalize_section_spacing(table);

	for (parent, section_header) in section_headers {
		attach_section_header(table, parent, &section_header);
	}

	clear_table_positions(table);
}

/// Put exactly one blank line before each table.
///
/// Table prefixes carry the whitespace and comments from their original
/// location. Reordering moves the comments with the table, but the original
/// blank-line count no longer matches the new neighbours.
fn normalize_section_spacing(table: &mut Table) {
	let keys = table
		.iter()
		.map(|(key, _)| key.to_string())
		.collect::<Vec<_>>();
	let mut first = true;

	for key in keys {
		let Some(item) = table.get_mut(&key) else {
			// patch-coverage:ignore-start -- keys come from the same table iterator, which skips Item::None.
			continue;
			// patch-coverage:ignore-end
		};

		match item {
			Item::Table(nested) => normalize_table_spacing(nested, &mut first),
			Item::ArrayOfTables(array) => {
				for nested in array.iter_mut() {
					normalize_table_spacing(nested, &mut first);
				}
			}
			Item::Value(_) | Item::None => {}
		}
	}
}

fn normalize_table_spacing(table: &mut Table, first: &mut bool) {
	normalize_prefix(table.decor_mut(), *first);
	*first = false;

	for (_, item) in table.iter_mut() {
		match item {
			Item::Table(nested) => normalize_table_spacing(nested, first),
			Item::ArrayOfTables(array) => {
				for nested in array.iter_mut() {
					normalize_table_spacing(nested, first);
				}
			}
			_ => {}
		}
	}
}

fn normalize_prefix(decor: &mut toml_edit::Decor, is_first: bool) {
	let prefix = decor
		.prefix()
		.and_then(|prefix| prefix.as_str())
		.unwrap_or_default();

	if is_first {
		decor.set_prefix(trim_leading_blank_lines(prefix));

		return;
	}

	let comments = comment_block(prefix);
	let prefix = if comments.is_empty() {
		"\n".to_string()
	} else {
		format!("\n{comments}")
	};

	decor.set_prefix(prefix);
}

fn trim_leading_blank_lines(value: &str) -> String {
	let mut remaining = value;

	while let Some((line, rest)) = remaining.split_once('\n') {
		if !line.trim().is_empty() {
			break;
		}

		remaining = rest;
	}

	remaining.to_string()
}

/// Keep comments and internal blank lines, but drop leading blank lines.
fn comment_block(prefix: &str) -> String {
	let lines = prefix.lines().collect::<Vec<_>>();
	let Some(start) = lines.iter().position(|line| !line.trim().is_empty()) else {
		return String::new();
	};
	let mut block = lines.get(start..).unwrap_or_default().join("\n");
	block.push('\n');

	block
}

/// Reorder the direct children of `table` using `order`.
///
/// Keys that are not listed in `order` keep their relative order after the
/// known keys.
fn reorder_table_keys(table: &mut Table, order: &[&str]) {
	let keys = table
		.iter()
		.map(|(key, _)| key.to_string())
		.collect::<Vec<_>>();
	let mut entries = Vec::with_capacity(keys.len());

	for key in keys {
		if let Some(entry) = table.remove_entry(&key) {
			entries.push(entry);
		}
	}

	entries.sort_by(|(left, _), (right, _)| compare_keys(left.get(), right.get(), order));

	for (key, item) in entries {
		table.insert_formatted(&key, item);
	}
}

fn compare_keys(left: &str, right: &str, order: &[&str]) -> Ordering {
	let left_rank = order.iter().position(|key| *key == left);
	let right_rank = order.iter().position(|key| *key == right);

	match (left_rank, right_rank) {
		(Some(left_rank), Some(right_rank)) => left_rank.cmp(&right_rank),
		(Some(_), None) => Ordering::Less,
		(None, Some(_)) => Ordering::Greater,
		(None, None) => Ordering::Equal,
	}
}

fn sort_child_tables(table: &mut Table, parent: &str) -> String {
	let Some(Item::Table(parent_table)) = table.get_mut(parent) else {
		return String::new();
	};

	let section_header = take_section_header(parent_table);
	let keys = parent_table
		.iter()
		.map(|(key, _)| key.to_string())
		.collect::<Vec<_>>();
	let mut entries = Vec::with_capacity(keys.len());

	for key in keys {
		if let Some(entry) = parent_table.remove_entry(&key) {
			entries.push(entry);
		}
	}

	entries.sort_by(|(left, _), (right, _)| left.get().cmp(right.get()));

	for (key, item) in entries {
		parent_table.insert_formatted(&key, item);
	}

	section_header
}

/// Move the comments before the first child out of that child.
///
/// A map-like section usually documents itself immediately before its first
/// child. Sorting would move those comments into the middle of the section
/// unless they are detached first.
fn take_section_header(table: &mut Table) -> String {
	let Some(first_key) = table.iter().map(|(key, _)| key.to_string()).next() else {
		return String::new();
	};
	let Some(Item::Table(first)) = table.get_mut(&first_key) else {
		return String::new();
	};
	let prefix = first
		.decor()
		.prefix()
		.and_then(|prefix| prefix.as_str())
		.unwrap_or_default();
	let (section_header, child_comments) = split_section_header(prefix);
	first.decor_mut().set_prefix(child_comments);

	section_header
}

/// Split the first child's prefix into a section header and child comments.
///
/// A section header is separated from the first child by a blank line. A
/// comment directly above the child belongs to that child instead.
fn split_section_header(prefix: &str) -> (String, String) {
	let lines = prefix.lines().collect::<Vec<_>>();
	let Some(last_blank) = lines.iter().rposition(|line| line.trim().is_empty()) else {
		return (String::new(), comment_block(prefix));
	};
	let header_lines = lines.get(..last_blank).unwrap_or_default();
	let section_header = comment_block(&header_lines.join("\n"));

	if section_header.is_empty() {
		return (String::new(), comment_block(prefix));
	}

	let child_lines = lines.get(last_blank + 1..).unwrap_or_default();
	let child_comments = comment_block(&child_lines.join("\n"));
	(section_header, child_comments)
}

fn attach_section_header(table: &mut Table, parent: &str, section_header: &str) {
	if section_header.is_empty() {
		return;
	}

	let Some(Item::Table(parent_table)) = table.get_mut(parent) else {
		return;
	};
	let Some(first_key) = parent_table.iter().map(|(key, _)| key.to_string()).next() else {
		return;
	};
	let Some(Item::Table(first)) = parent_table.get_mut(&first_key) else {
		return;
	};
	let prefix = first
		.decor()
		.prefix()
		.and_then(|prefix| prefix.as_str())
		.unwrap_or_default();
	let comments = comment_block(prefix);
	let prefix = if comments.is_empty() {
		format!("\n{section_header}\n")
	} else {
		format!("\n{section_header}\n{comments}")
	};

	first.decor_mut().set_prefix(prefix);
}

fn sort_group_packages(table: &mut Table) {
	let Some(Item::Table(groups)) = table.get_mut("group") else {
		return;
	};

	for (_, item) in groups.iter_mut() {
		let Item::Table(group) = item else {
			continue;
		};
		let Some(Item::Value(Value::Array(packages))) = group.get_mut("packages") else {
			continue;
		};

		sort_string_array(packages);
	}
}

fn sort_string_array(array: &mut Array) {
	if array.iter().any(|value| value.as_str().is_none()) {
		return;
	}

	array.sort_by(|left, right| {
		left.as_str()
			.unwrap_or_default()
			.cmp(right.as_str().unwrap_or_default())
	});
}

/// Clear source positions so `toml_edit` renders tables in insertion order.
///
/// Parsed tables retain their original document positions. Without this reset,
/// moving a table in the map would not move it in the rendered document.
fn clear_table_positions(table: &mut Table) {
	table.set_position(None);

	for (_, item) in table.iter_mut() {
		match item {
			Item::Table(nested) => clear_table_positions(nested),
			Item::ArrayOfTables(array) => {
				for nested in array.iter_mut() {
					clear_table_positions(nested);
				}
			}
			_ => {}
		}
	}
}

fn formatter_options() -> taplo::formatter::Options {
	taplo::formatter::Options {
		align_comments: false,
		align_entries: false,
		array_auto_collapse: false,
		array_auto_expand: true,
		array_trailing_comma: true,
		column_width: 100,
		compact_arrays: true,
		compact_entries: false,
		compact_inline_tables: false,
		indent_entries: false,
		indent_string: "\t".to_string(),
		indent_tables: false,
		inline_table_expand: false,
		reorder_arrays: false,
		reorder_inline_tables: false,
		reorder_keys: false,
		trailing_newline: true,
		allowed_blank_lines: 1,
		..taplo::formatter::Options::default()
	}
}

#[cfg(test)]
#[path = "__tests__/format_tests.rs"]
mod tests;
