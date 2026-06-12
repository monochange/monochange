use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;

/// Initialize the tracing subscriber for CLI diagnostics.
///
/// Priority:
/// 1. `log_level` parameter from `--log-level` CLI flag
/// 2. No subscriber installed (silent, near-zero overhead)
///
/// Note: `EnvFilter` is intentionally not used. It pulls in the `tracing-log`
/// crate and regex-based directive parsing (~1.4 MiB in the release binary).
/// A simple `LevelFilter` covers the CLI use case with zero extra weight.
pub(crate) fn init_tracing(log_level: Option<&str>) {
	let level = match log_level {
		Some(level) => level.parse::<LevelFilter>().unwrap_or(LevelFilter::INFO),
		None => return,
	};

	let subscriber = fmt::Subscriber::builder()
		.with_max_level(level)
		.with_span_events(FmtSpan::CLOSE)
		.with_target(true)
		.with_writer(std::io::stderr)
		.finish();

	let _ = tracing::subscriber::set_global_default(subscriber);
}
