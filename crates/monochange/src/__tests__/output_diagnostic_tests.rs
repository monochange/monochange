use std::io;
use std::path::Path;
use std::path::PathBuf;

use super::*;

#[test]
fn configuration_diagnostic_has_a_stable_code_context_and_hint() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Config("missing release output".to_string()),
		Some("monochange step prepare-release"),
	);
	assert_eq!(
		diagnostic.render(false),
		"error[config.invalid]: missing release output\n  command: monochange step prepare-release\n  help:    Check `monochange.toml` and the command arguments, then rerun the command."
	);
}

#[test]
fn file_diagnostic_names_the_affected_path() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::IoSource {
			path: PathBuf::from("monochange.toml"),
			source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
		},
		Some("monochange check"),
	);
	let rendered = diagnostic.render(false);
	assert!(rendered.starts_with("error[file.io_failed]: denied"));
	assert!(rendered.contains("path:    monochange.toml"));
	assert!(rendered.contains("help:"));
}

#[test]
fn diagnostic_color_is_scoped_and_reset() {
	let rendered = CliDiagnostic::from_error(&MonochangeError::Cancelled, None).render(true);
	assert!(rendered.starts_with("\u{1b}[31;1merror[operation.cancelled]\u{1b}[0m"));
	assert!(rendered.contains("\u{1b}[36;1mhelp:\u{1b}[0m"));
}

#[test]
fn multiline_causes_render_as_an_indented_block_without_trailing_whitespace() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Config("invalid command\n\nUsage: monochange check\n".to_string()),
		None,
	);
	let rendered = diagnostic.render(false);

	assert_eq!(
		rendered,
		"error[config.invalid]: invalid command\n\n    Usage: monochange check\n\n  help: Check `monochange.toml` and the command arguments, then rerun the command.",
	);
	assert!(rendered.lines().all(|line| line.trim_end() == line));
}

#[test]
fn command_failures_explain_the_output_and_name_the_failed_step() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Discovery(
			"command `cargo test` failed: exit status: 101\nstderr:\ntest a ... FAILED\n\nmore"
				.to_string(),
		),
		Some("monochange run release"),
	)
	.with_step("[3/4] run tests".to_string());

	insta::assert_snapshot!(diagnostic.render(false));
	assert_eq!(
		diagnostic.annotation_title(),
		"monochange run release failed"
	);
	insta::assert_snapshot!("command_failure_annotation", diagnostic.annotation());
}

#[test]
fn pre_rendered_source_diagnostics_keep_their_snippet_alignment() {
	let message = "error: package `zzz` path `crates/missing` does not exist\n  --> monochange.toml:39:1\n\n   |\n39 | path = \"crates/missing\"\n   | ^^^^^^ missing package path\n\n  = help: create the package directory\n  = note: paths are relative";
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Diagnostic(message.to_string()),
		Some("monochange next"),
	);

	insta::assert_snapshot!("source_snippet_plain", diagnostic.render(false));
	insta::assert_snapshot!("source_snippet_colored", diagnostic.render(true));
	assert_eq!(diagnostic.annotation_title(), "monochange next failed");
}

#[test]
fn usage_errors_keep_the_usage_text_and_drop_the_config_hint() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Diagnostic(
			"error: unrecognized subcommand 'relase'\n\n  tip: some similar subcommands exist: 'prepare'\n\nUsage: monochange [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.".to_string(),
		),
		Some("monochange relase"),
	);

	insta::assert_snapshot!(diagnostic.render(false));
	assert_eq!(
		diagnostic_code("error: invalid value 'yaml'\n\nFor more information, try '--help'."),
		"cli.usage"
	);
	assert_eq!(
		diagnostic_code("changeset target validation failed:\nerror: x"),
		"changeset.invalid"
	);
	assert_eq!(diagnostic_code("error: bad"), "config.invalid");
	assert_eq!(diagnostic_code("plain"), "cli.diagnostic");
}

#[test]
fn workspace_paths_render_relative_to_the_root() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Config(
			"failed to parse /repo/monochange.toml: TOML parse error\n  --> /repo/monochange.toml:2:1"
				.to_string(),
		),
		Some("monochange next"),
	)
	.with_root(Path::new("/repo"));

	assert_eq!(
		diagnostic.render(false),
		"error[config.parse_failed]: failed to parse monochange.toml: TOML parse error\n\n      --> monochange.toml:2:1\n\n  command: monochange next\n  help:    Fix the syntax error shown above, then rerun the command."
	);
	let unchanged = CliDiagnostic::from_error(
		&MonochangeError::Config("repo/monochange.toml".to_string()),
		None,
	)
	.with_root(Path::new("repo"));
	assert!(
		unchanged
			.render(false)
			.starts_with("error[config.invalid]: repo/monochange.toml")
	);
}

#[test]
fn steps_follow_the_command_context_or_lead_without_one() {
	let with_command = CliDiagnostic::from_error(
		&MonochangeError::Diagnostic("failed".to_string()),
		Some("monochange run release"),
	)
	.with_step("prepare release".to_string());
	assert_eq!(
		with_command.render(false),
		"error[cli.diagnostic]: failed\n  command: monochange run release\n  step:    prepare release"
	);
	assert_eq!(
		with_command.annotation(),
		"error[cli.diagnostic]: failed\nstep: prepare release"
	);

	let without_command =
		CliDiagnostic::from_error(&MonochangeError::Diagnostic("failed".to_string()), None)
			.with_step("prepare release".to_string());
	assert_eq!(
		without_command.render(false),
		"error[cli.diagnostic]: failed\n  step: prepare release"
	);
	assert_eq!(without_command.annotation_title(), "monochange failed");
}

#[test]
fn diagnostics_cover_every_error_category_and_sanitize_terminal_controls() {
	let parse_source = serde_json::from_str::<serde_json::Value>("{")
		.expect_err("invalid JSON should create a parse error");
	let errors = [
		MonochangeError::Io("disk full".to_string()),
		MonochangeError::Config("--jq requires explicit JSON output".to_string()),
		MonochangeError::Config("check failed: 2 errors".to_string()),
		MonochangeError::Config(
			"change package reference `x` did not match any discovered package".to_string(),
		),
		MonochangeError::Config("failed to parse monochange.toml".to_string()),
		MonochangeError::Discovery("package missing".to_string()),
		MonochangeError::Discovery("command `make` failed: exit status: 2".to_string()),
		MonochangeError::Discovery("no monochange release record found from `HEAD`".to_string()),
		MonochangeError::Discovery(
			"package publishing did not complete: 1 total, 1 failed".to_string(),
		),
		MonochangeError::Discovery(
			"could not resolve ref `v1` to a commit: fatal: Needed a single revision".to_string(),
		),
		MonochangeError::Diagnostic("raw diagnostic".to_string()),
		MonochangeError::Reported {
			output: "report".to_string(),
			diagnostic: "check failed: 1 error".to_string(),
		},
		MonochangeError::Reported {
			output: "report".to_string(),
			diagnostic: "publish failed".to_string(),
		},
		MonochangeError::Parse {
			path: PathBuf::from("changeset.json"),
			source: Box::new(parse_source),
		},
		MonochangeError::Interactive {
			message: "input unavailable".to_string(),
		},
	];

	let rendered = errors
		.iter()
		.map(|error| CliDiagnostic::from_error(error, None).render(false))
		.collect::<Vec<_>>();
	for code in [
		"io.failed",
		"cli.json_required",
		"check.failed",
		"config.unknown_package",
		"config.parse_failed",
		"workspace.discovery_failed",
		"release.record_failed",
		"publish.failed",
		"git.failed",
		"step.command_failed",
		"cli.diagnostic",
		"command.failed",
		"file.parse_failed",
		"interactive.failed",
	] {
		assert!(
			rendered
				.iter()
				.any(|value| value.starts_with(&format!("error[{code}]"))),
			"missing {code} in {rendered:#?}"
		);
	}
	assert_eq!(
		rendered
			.iter()
			.find(|value| value.contains("publish failed"))
			.map(String::as_str),
		Some("error[command.failed]: publish failed")
	);

	let controlled = CliDiagnostic::from_error(
		&MonochangeError::Config("\u{1b}[31mbad\u{1b}[0m\r\n\u{1b}[33mcause\u{1b}[0m".to_string()),
		Some("monochange check"),
	)
	.render(false);
	assert_eq!(
		controlled,
		"error[config.invalid]: bad\n\n    cause\n\n  command: monochange check\n  help:    Check `monochange.toml` and the command arguments, then rerun the command.",
	);
	let unknown_escape =
		CliDiagnostic::from_error(&MonochangeError::Diagnostic("a\u{1b}xb".to_string()), None)
			.render(false);
	assert_eq!(unknown_escape, "error[cli.diagnostic]: ab");
}

#[cfg(feature = "github")]
#[test]
fn http_diagnostic_uses_the_generic_command_failure_category() {
	monochange_test_helpers::install_rustls_ring_provider();
	let source = reqwest::Client::new()
		.get("http://[::1")
		.build()
		.expect_err("invalid URL should fail while building the request");
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::HttpRequest {
			context: "fetch releases".to_string(),
			source,
		},
		None,
	);

	assert!(
		diagnostic
			.render(false)
			.starts_with("error[command.failed]:")
	);
}

#[test]
fn colored_context_preserves_embedded_styling_and_multiline_layout() {
	let diagnostic = CliDiagnostic {
		code: "test.failed",
		summary: "summary".to_string(),
		body: None,
		context: vec![("cause", "first\n\n\u{1b}[33msecond\u{1b}[0m".to_string())],
		hints: vec!["retry".to_string()],
	};
	let rendered = diagnostic.render(true);

	assert!(rendered.contains("  \u{1b}[36;1mcause:\u{1b}[0m first\n\n"));
	assert!(rendered.contains("         \u{1b}[33msecond\u{1b}[0m"));
}

#[test]
fn generic_hints_are_skipped_when_the_message_already_says_what_to_do() {
	let guided = CliDiagnostic::from_error(
		&MonochangeError::Config(
			"tag `v1` already points elsewhere; use `monochange step retarget-release`".to_string(),
		),
		None,
	);
	assert_eq!(
		guided.render(false),
		"error[config.invalid]: tag `v1` already points elsewhere; use `monochange step retarget-release`"
	);
	let upgrade = CliDiagnostic::from_error(
		&MonochangeError::Discovery(
			"release record uses unsupported schema_version 9 (upgrade monochange)".to_string(),
		),
		None,
	);
	assert_eq!(
		upgrade.render(false),
		"error[release.record_failed]: release record uses unsupported schema_version 9 (upgrade monochange)"
	);
	let git = CliDiagnostic::from_error(
		&MonochangeError::Discovery("git command failed: rev-list".to_string()),
		None,
	);
	assert!(git.render(false).contains("fetch-depth: 0"));
}

#[test]
fn snippet_colors_only_apply_to_snippet_structure() {
	assert_eq!(colorize_snippet_line("plain text", true), "plain text");
	assert_eq!(colorize_snippet_line("a | b", true), "a | b");
	assert_eq!(colorize_snippet_line("  --> x", false), "  --> x");
	assert_eq!(
		colorize_snippet_line("12 | - item", true),
		"\u{1b}[36;1m12 |\u{1b}[0m - item"
	);
}
