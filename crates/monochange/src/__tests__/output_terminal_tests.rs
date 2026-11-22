use std::ffi::OsString;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;

use super::*;

fn probe() -> TerminalProbe {
	TerminalProbe {
		stdout_is_terminal: true,
		stderr_is_terminal: true,
		ci: false,
		github_actions: false,
		color_environment: crate::output::color::ColorEnvironment::default(),
		no_progress: false,
	}
}

#[test]
fn progress_settings_parse_environment_independent_cli_values() {
	let settings = ProgressSettings::from_args(&[
		OsString::from("monochange"),
		OsString::from("--quiet"),
		OsString::from("--progress-format=ascii"),
	]);
	assert!(settings.quiet);
	assert_eq!(settings.format, ProgressFormat::Ascii);
	assert!(!settings.tracing_enabled);

	let settings = ProgressSettings::from_args(&[
		OsString::from("monochange"),
		OsString::from("--progress-format"),
		OsString::from("json"),
		OsString::from("--log-level=debug"),
	]);
	assert_eq!(settings.format, ProgressFormat::Json);
	assert!(settings.tracing_enabled);
	assert!(!settings.verbose);

	let settings = ProgressSettings::from_args(&[
		OsString::from("monochange"),
		OsString::from("-v"),
		OsString::from("next"),
	]);
	assert!(settings.verbose);
	assert!(TerminalCapabilities::resolve(settings, probe()).verbose);
}

#[test]
fn environment_probes_recognize_ci_and_test_markers() {
	temp_env::with_vars(
		[
			("CI", None::<&str>),
			("GITHUB_ACTIONS", None::<&str>),
			("GITLAB_CI", None::<&str>),
			("BUILDKITE", None::<&str>),
			("CIRCLECI", None::<&str>),
			("TF_BUILD", None::<&str>),
			("CARGO_NEXTEST", None::<&str>),
			("NEXTEST", None::<&str>),
			("INSTA_WORKSPACE_ROOT", None::<&str>),
			("INSTA_UPDATE", None::<&str>),
		],
		|| {
			assert!(!running_in_ci());
			assert!(!running_in_github_actions());
			assert!(!running_under_test());
		},
	);
	temp_env::with_var("GITHUB_ACTIONS", Some("true"), || {
		assert!(running_in_github_actions());
	});
	temp_env::with_var("GITHUB_ACTIONS", Some("false"), || {
		assert!(!running_in_github_actions());
	});
	temp_env::with_vars(
		[("TF_BUILD", Some("1")), ("INSTA_UPDATE", Some("no"))],
		|| {
			assert!(running_in_ci());
			assert!(running_under_test());
		},
	);
}
