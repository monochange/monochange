use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::output::warnings::WarningSink;
use crate::output::warnings::warning_subscriber;

/// Initialize the tracing subscriber for CLI diagnostics.
///
/// Priority:
/// 1. `log_level` parameter from `--log-level` CLI flag installs the full
///    maintainer trace, which includes warnings.
/// 2. Otherwise `warnings` installs a subscriber that renders only `WARN`
///    events as readable `warning:` lines, so users see fallbacks and retries
///    without opting into a trace. `None` (for `--quiet`) installs nothing.
///
/// Note: `EnvFilter` is intentionally not used. It pulls in the `tracing-log`
/// crate and regex-based directive parsing (~1.4 MiB in the release binary).
/// A simple `LevelFilter` covers the CLI use case with zero extra weight.
pub(crate) fn init_tracing(log_level: Option<&str>, warnings: Option<WarningSink>) {
	let Some(level) = log_level else {
		if let Some(sink) = warnings {
			let _ = tracing::subscriber::set_global_default(warning_subscriber(sink));
		}
		return;
	};
	let level = level.parse::<LevelFilter>().unwrap_or(LevelFilter::INFO);

	let subscriber = fmt::Subscriber::builder()
		.with_max_level(level)
		.with_span_events(FmtSpan::CLOSE)
		.with_target(true)
		.with_writer(std::io::stderr)
		.finish();

	let _ = tracing::subscriber::set_global_default(subscriber);
}
