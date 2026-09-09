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
		term_is_dumb: false,
		no_color: false,
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
}

#[test]
fn terminal_capabilities_resolve_one_consistent_policy() {
	let settings = ProgressSettings {
		quiet: false,
		format: ProgressFormat::Auto,
		tracing_enabled: false,
	};
	let interactive = TerminalCapabilities::resolve(settings, probe());
	assert!(interactive.stdout_is_terminal);
	assert!(interactive.stderr_is_terminal);
	assert!(!interactive.quiet);
	assert!(interactive.color);
	assert!(interactive.animate);
	assert!(interactive.progress_enabled);

	let redirected = TerminalCapabilities::resolve(
		settings,
		TerminalProbe {
			stderr_is_terminal: false,
			..probe()
		},
	);
	assert!(!redirected.color);
	assert!(!redirected.animate);
	assert!(redirected.progress_enabled);

	let ci = TerminalCapabilities::resolve(
		settings,
		TerminalProbe {
			ci: true,
			..probe()
		},
	);
	assert!(ci.color);
	assert!(!ci.animate);

	let tracing = TerminalCapabilities::resolve(
		ProgressSettings {
			quiet: false,
			format: ProgressFormat::Auto,
			tracing_enabled: true,
		},
		probe(),
	);
	assert!(tracing.color);
	assert!(!tracing.animate);

	let disabled = TerminalCapabilities::resolve(
		settings,
		TerminalProbe {
			no_progress: true,
			..probe()
		},
	);
	assert!(!disabled.progress_enabled);
	assert!(!disabled.color);
	assert!(!disabled.animate);

	let quiet = TerminalCapabilities::resolve(
		ProgressSettings {
			quiet: true,
			..settings
		},
		probe(),
	);
	assert!(quiet.quiet);
}

#[test]
fn explicit_json_progress_never_uses_terminal_styling() {
	let capabilities = TerminalCapabilities::resolve(
		ProgressSettings {
			quiet: false,
			format: ProgressFormat::Json,
			tracing_enabled: false,
		},
		probe(),
	);
	assert!(!capabilities.color);
	assert!(!capabilities.animate);
}

#[derive(Clone)]
struct RecordedWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for RecordedWriter {
	fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
		self.0.lock().unwrap().extend_from_slice(bytes);
		Ok(bytes.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

#[test]
fn shared_stderr_serializes_cloned_writers() {
	let bytes = Arc::new(Mutex::new(Vec::new()));
	let writer = SharedStderr::with_writer(RecordedWriter(Arc::clone(&bytes)));
	writer.write(b"first\n");
	writer.clone().write(b"second\n");

	assert_eq!(bytes.lock().unwrap().as_slice(), b"first\nsecond\n");
}

#[test]
fn shared_stderr_ignores_a_poisoned_writer_lock() {
	let writer = SharedStderr::with_writer(io::sink());
	let poisoned = writer.clone();
	let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
		let _guard = poisoned.writer.lock().unwrap();
		panic!("poison writer lock");
	}));

	writer.write(b"ignored");
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
			assert!(!running_under_test());
		},
	);
	temp_env::with_vars(
		[("TF_BUILD", Some("1")), ("INSTA_UPDATE", Some("no"))],
		|| {
			assert!(running_in_ci());
			assert!(running_under_test());
		},
	);
}
