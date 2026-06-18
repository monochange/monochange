use super::markdown_to_clap_help;
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
fn strip_inline_markdown_preserves_unlinked_labels() {
	assert_eq!(
		strip_inline_markdown("Use [label] and `code`"),
		"Use label and code"
	);
}
