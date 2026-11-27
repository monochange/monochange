use monochange_classification::ClassifyOptions;
use monochange_classification::DependencyPropagation;
use monochange_classification::parse_dependency_propagation;
use monochange_classification::parse_detection_level;
use monochange_core::DetectionLevel;

use super::markdown_to_clap_help;
use super::step_command_summary;
use super::strip_inline_markdown;

#[test]
fn markdown_to_clap_help_normalizes_headings_links_and_code_blocks() {
	let help = markdown_to_clap_help(
		"# Root `monochange`\n\n## Workflow [guide](https://example.com)\n\n### Details\n\nRun `monochange --help`.\n\n```\nmonochange run release --help\n```\n",
	);

	assert_eq!(
		help,
		"Root monochange\n\nWorkflow guide\n\nDetails\n\nRun monochange --help.\n\n  monochange run release --help",
	);
}

#[test]
fn classify_options_cover_all_supported_formats_and_detection_levels() {
	for (format, expected_format) in [
		(
			"markdown",
			monochange_classification::ClassificationFormat::Markdown,
		),
		(
			"md",
			monochange_classification::ClassificationFormat::Markdown,
		),
		(
			"json",
			monochange_classification::ClassificationFormat::Json,
		),
		(
			"json-min",
			monochange_classification::ClassificationFormat::JsonMin,
		),
		(
			"text",
			monochange_classification::ClassificationFormat::Text,
		),
	] {
		let matches = crate::cli::build_command_with_cli("monochange", &[])
			.try_get_matches_from(["monochange", "change", "classify", "--format", format])
			.unwrap_or_else(|error| panic!("parse {format}: {error}"));
		let (_, change_matches) = matches.subcommand().unwrap();
		let (_, classify_matches) = change_matches.subcommand().unwrap();
		let options = crate::cli::classify_options_from_matches(classify_matches)
			.unwrap_or_else(|error| panic!("extract {format}: {error}"));
		assert_eq!(options.format, expected_format);
	}

	assert_eq!(
		parse_detection_level("basic").unwrap(),
		DetectionLevel::Basic
	);
	assert_eq!(
		parse_detection_level("semantic").unwrap(),
		DetectionLevel::Semantic
	);
	assert!(parse_detection_level("impossible").is_err());
	assert_eq!(
		parse_dependency_propagation("none").unwrap(),
		DependencyPropagation::None
	);
	assert_eq!(
		parse_dependency_propagation("public").unwrap(),
		DependencyPropagation::Public
	);
	assert!(parse_dependency_propagation("transitive").is_err());
}
