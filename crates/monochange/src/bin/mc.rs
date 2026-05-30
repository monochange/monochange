#![allow(unstable_features)]
#![allow(clippy::large_futures)]
#![feature(coverage_attribute)]

use std::process::ExitCode;

/// Synchronous fast path for --version that skips all async initialization.
/// This avoids Tokio runtime, rustls crypto provider, and tracing setup.
#[coverage(off)]
fn try_sync_version() -> Option<ExitCode> {
	let args: Vec<String> = std::env::args().collect();
	if args.get(1).is_some_and(|a| a == "--version" || a == "-V") {
		println!("mc {}", env!("CARGO_PKG_VERSION"));
		return Some(ExitCode::SUCCESS);
	}
	None
}

#[coverage(off)]
#[allow(clippy::disallowed_methods)]
fn main() -> ExitCode {
	if let Some(code) = try_sync_version() {
		return code;
	}
	tokio_main()
}

#[coverage(off)]
#[allow(clippy::disallowed_methods)]
#[tokio::main(flavor = "current_thread")]
async fn tokio_main() -> ExitCode {
	monochange::run_cli_binary_from_env("mc").await
}
