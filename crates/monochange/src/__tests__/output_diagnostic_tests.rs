use std::io;
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
		"error[config.invalid]: missing release output\n  command: monochange step prepare-release\n  help: Check `monochange.toml` and the command arguments, then rerun the command."
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
	assert!(rendered.contains("path: monochange.toml"));
	assert!(rendered.contains("help:"));
}

#[test]
fn diagnostic_color_is_scoped_and_reset() {
	let rendered = CliDiagnostic::from_error(&MonochangeError::Cancelled, None).render(true);
	assert!(rendered.starts_with("\u{1b}[31;1merror[operation.cancelled]\u{1b}[0m"));
	assert!(rendered.contains("\u{1b}[36;1mhelp\u{1b}[0m"));
}

#[test]
fn multiline_causes_do_not_leave_whitespace_on_blank_lines() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Config("invalid command\n\nUsage: monochange check".to_string()),
		None,
	);
	let rendered = diagnostic.render(false);

	assert_eq!(
		rendered,
		"error[config.invalid]: invalid command\n  cause: Usage: monochange check\n  help: Check `monochange.toml` and the command arguments, then rerun the command.",
	);
	assert!(rendered.lines().all(|line| line.trim_end() == line));
}

#[test]
fn diagnostics_cover_every_error_category_and_sanitize_terminal_controls() {
	let parse_source = serde_json::from_str::<serde_json::Value>("{")
		.expect_err("invalid JSON should create a parse error");
	let errors = [
		MonochangeError::Io("disk full".to_string()),
		MonochangeError::Config("--jq requires explicit JSON output".to_string()),
		MonochangeError::Config("check failed: 2 errors".to_string()),
		MonochangeError::Discovery("package missing".to_string()),
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
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[io.failed]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[cli.json_required]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[check.failed]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[workspace.discovery_failed]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[cli.diagnostic]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[command.failed]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[file.parse_failed]"))
	);
	assert!(
		rendered
			.iter()
			.any(|value| value.contains("error[interactive.failed]"))
	);

	let controlled = CliDiagnostic::from_error(
		&MonochangeError::Config("\u{1b}[31mbad\u{1b}[0m\r\n\u{1b}[33mcause\u{1b}[0m".to_string()),
		Some("monochange check"),
	)
	.render(false);
	assert_eq!(
		controlled,
		"error[config.invalid]: bad\n  cause: cause\n  command: monochange check\n  help: Check `monochange.toml` and the command arguments, then rerun the command.",
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
		context: vec![("cause", "first\n\n\u{1b}[33msecond\u{1b}[0m".to_string())],
		hints: vec!["retry".to_string()],
	};
	let rendered = diagnostic.render(true);

	assert!(rendered.contains("  \u{1b}[36;1mcause\u{1b}[0m: first\n\n"));
	assert!(rendered.contains("    \u{1b}[33msecond\u{1b}[0m"));
}
