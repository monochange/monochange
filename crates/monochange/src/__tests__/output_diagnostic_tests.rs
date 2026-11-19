use std::io;
use std::path::Path;
use std::path::PathBuf;

use super::*;

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
fn rate_limited_publishes_say_when_to_rerun_instead_of_a_generic_hint() {
	let diagnostic = CliDiagnostic::from_error(
		&MonochangeError::Discovery(
			"crates.io rate limit stopped publishing at canvist_layout 0.0.0: 11 of 15 packages \
			 published\ncrates.io: You have published too many new crates in a short period of \
			 time.\nretry after: 10:59:10 UTC (in 6 minutes)\nlimits: \
			 https://crates.io/docs/rate-limits\nhelp: Wait until 10:59:10 UTC (in 6 minutes), then \
			 rerun the same command."
				.to_string(),
		),
		Some("monochange publish placeholder"),
	);

	insta::assert_snapshot!(diagnostic.render(false), @r"
	error[publish.rate_limited]: crates.io rate limit stopped publishing at canvist_layout 0.0.0: 11 of 15 packages published

	    crates.io: You have published too many new crates in a short period of time.
	    retry after: 10:59:10 UTC (in 6 minutes)
	    limits: https://crates.io/docs/rate-limits

	  command: monochange publish placeholder
	  help:    Wait until 10:59:10 UTC (in 6 minutes), then rerun the same command.
	");
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
fn hosted_request_failures_are_not_reported_as_config_errors() {
	let oidc = CliDiagnostic::from_error(
		&MonochangeError::Config(
			"hosted CommitRelease OIDC token request failed with status 503 Service Unavailable \
			 after 4 attempts. GitHub Actions could not issue the job's OIDC token, which is a \
			 GitHub outage rather than a problem with this workspace; rerun the job."
				.to_string(),
		),
		None,
	)
	.render(false);
	assert!(
		oidc.starts_with("error[hosted.request_failed]: hosted CommitRelease OIDC token"),
		"{oidc}"
	);
	assert!(!oidc.contains("monochange.toml"), "{oidc}");

	let conflict = CliDiagnostic::from_error(
		&MonochangeError::Config(
			"hosted CommitRelease failed with HTTP 409 Conflict: base branch moved".to_string(),
		),
		None,
	)
	.render(false);
	assert_eq!(
		conflict,
		"error[hosted.request_failed]: hosted CommitRelease failed with HTTP 409 Conflict: base \
		 branch moved"
	);

	let unreachable = CliDiagnostic::from_error(
		&MonochangeError::Config(
			"hosted CommitRelease request failed: connection refused".to_string(),
		),
		None,
	)
	.render(false);
	assert_eq!(
		unreachable,
		"error[hosted.request_failed]: hosted CommitRelease request failed: connection \
		 refused\n  help: Rerun the job. If it fails again, check that the job grants \
		 `id-token: write` and that the monochange GitHub App is installed on the repository."
	);

	let missing_token = CliDiagnostic::from_error(
		&MonochangeError::Config(
			"hosted CommitRelease token auth requires MONOCHANGE_TOKEN".to_string(),
		),
		None,
	)
	.render(false);
	assert!(
		missing_token.starts_with("error[config.invalid]"),
		"{missing_token}"
	);
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
