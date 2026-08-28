#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]

//! # `monochange_test_helpers`
//!
//! `monochange_test_helpers` packages the shared fixture, snapshot, git, and RMCP helpers used across the workspace test suite.
//!
//! Reach for this crate when you are writing integration or fixture-heavy tests that need scenario workspaces, command snapshots, or temporary git repositories.
//!
//! ## Why use it?
//!
//! - keep tests focused on behavior instead of tempdir and setup boilerplate
//! - share consistent fixture loading across crates
//! - reuse snapshot and git helpers in integration suites
//!
//! ## Best for
//!
//! - copying fixture workspaces into temp directories
//! - writing git-backed integration tests
//! - configuring `insta` snapshots and RMCP content assertions
//!
//! ## Public entry points
//!
//! - `copy_directory` and `copy_directory_skip_git` clone fixture trees into temp workspaces
//! - `git`, `git_output`, and `git_output_trimmed` run test git commands
//! - `snapshot_settings()` configures shared snapshot behavior
//! - `fixture_path!`, `setup_fixture!`, and `setup_scenario_workspace!` locate and materialize test fixtures
pub mod fs;
pub mod git;
pub mod insta;
pub mod lint_testing;
pub mod rmcp;
pub mod workspace_ops;

pub use fs::copy_directory;
pub use fs::copy_directory_skip_git;
pub use fs::current_test_name;
pub use git::git;
pub use git::git_output;
pub use git::git_output_trimmed;
pub use insta::snapshot_settings;
pub use rmcp::content_text;

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;

/// Resolve a workspace binary for tests, building it when Cargo did not expose
/// `CARGO_BIN_EXE_<name>` to the current test crate.
pub fn get_cargo_bin(name: &str) -> std::path::PathBuf {
	let env_name = format!("CARGO_BIN_EXE_{}", name.replace('-', "_"));
	// patch-coverage:ignore-start -- cargo-owned test binary env injection and platform suffixes are runner-specific; fallback resolution is covered by integration tests.
	if let Some(path) = std::env::var_os(&env_name) {
		return path.into();
	}

	let binary_name = if cfg!(windows) {
		format!("{name}.exe")
	} else {
		name.to_owned()
	};
	// patch-coverage:ignore-end
	let mut current_exe = std::env::current_exe()
		.unwrap_or_else(|error| panic!("resolve current test executable path: {error}"));
	while let Some(file_name) = current_exe.file_name().and_then(|value| value.to_str()) {
		if file_name == "debug" || file_name == "release" {
			break;
		}
		current_exe.pop();
	}
	let binary_path = current_exe.join(&binary_name);
	if binary_path.exists() {
		return binary_path;
	}

	let target_dir = current_exe
		.parent()
		.unwrap_or_else(|| panic!("resolve target directory for `{name}` binary tests"));
	let status = std::process::Command::new("cargo")
		.args(["build", "-p", name, "--bin", name, "--target-dir"])
		.arg(target_dir)
		.status()
		.unwrap_or_else(|error| panic!("build `{name}` binary for tests: {error}"));
	assert!(status.success(), "build `{name}` binary for tests");
	assert!(binary_path.exists());
	binary_path
}

/// Install the ring crypto provider as the default for rustls.
///
/// Required because monochange uses `rustls-no-provider` with reqwest.
/// Without this, any HTTPS request will panic with "No provider set".
///
/// Safe to call multiple times; subsequent calls are no-ops.
pub fn install_rustls_ring_provider() {
	static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
	INIT.get_or_init(|| {
		let _ = rustls::crypto::ring::default_provider().install_default();
	});
}

#[macro_export]
macro_rules! fixture_path {
	($relative:expr) => {
		$crate::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), $relative)
	};
}

#[macro_export]
macro_rules! setup_fixture {
	($relative:expr) => {
		$crate::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), $relative)
	};
}

#[macro_export]
macro_rules! setup_scenario_workspace {
	($relative:expr) => {
		$crate::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), $relative)
	};
}
