//! Small helpers that read typed values out of `clap` matches.
//!
//! Each subcommand parses the same handful of shapes (repo id overrides, auth
//! tokens, formats, duration thresholds), so the parsing lives here instead of
//! being repeated in every command module.

use clap::ArgMatches;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use crate::changeset_policy::normalize_changed_path;

/// Return the `--repo-id` override when it is present.
pub(super) fn repo_id_override(matches: &ArgMatches) -> Option<&str> {
	matches.get_one::<String>("repo-id").map(String::as_str)
}

/// Return the explicit `--token` value when it is present.
pub(super) fn explicit_token(matches: &ArgMatches) -> Option<&str> {
	matches.get_one::<String>("token").map(String::as_str)
}

/// Return the requested output format, defaulting to plain text.
pub(super) fn format_arg(matches: &ArgMatches) -> &str {
	matches
		.get_one::<String>("format")
		.map_or("text", String::as_str)
}

/// Read a repeated string argument as an owned vector.
pub(super) fn collect_strings(matches: &ArgMatches, arg: &str) -> Vec<String> {
	matches
		.get_many::<String>(arg)
		.map(|values| values.cloned().collect())
		.unwrap_or_default()
}

/// Read repeated paths and normalize them to workspace-relative form.
pub(super) fn collect_normalized_paths(matches: &ArgMatches, arg: &str) -> Vec<String> {
	matches
		.get_many::<String>(arg)
		.map(|values| {
			values
				.map(|value| normalize_changed_path(value))
				.filter(|value| !value.is_empty())
				.collect()
		})
		.unwrap_or_default()
}

/// Read a required string argument, reporting the flag name when it is missing.
pub(super) fn required_arg(matches: &ArgMatches, arg: &str) -> MonochangeResult<String> {
	matches
		.get_one::<String>(arg)
		.cloned()
		.ok_or_else(|| MonochangeError::Config(format!("missing `--{arg}`")))
}

/// Read a `u64` minute threshold, falling back to the supplied default.
pub(super) fn duration_minutes_arg(matches: &ArgMatches, arg: &str, default: u64) -> u64 {
	matches.get_one::<u64>(arg).copied().unwrap_or(default)
}

/// Combine an explicit CLI value with an environment fallback.
pub(super) fn cli_or_env(matches: &ArgMatches, arg: &str, env_name: &str) -> Option<String> {
	matches
		.get_one::<String>(arg)
		.cloned()
		.or_else(|| env_value(env_name))
}

/// Read a non-empty environment variable.
pub(super) fn env_value(name: &str) -> Option<String> {
	std::env::var(name).ok().filter(|value| !value.is_empty())
}
