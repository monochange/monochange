#![allow(unstable_features)]
#![allow(clippy::large_futures)]
#![feature(coverage_attribute)]

use std::process::ExitCode;

#[coverage(off)]
#[allow(clippy::disallowed_methods)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
	monochange::run_cli_binary_from_env("monochange").await
}
