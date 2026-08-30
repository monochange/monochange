use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use serde_json::Value;
use tempfile::TempDir;

const DUPLICATE_VERSION_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"versions\":{\"1.0.0\":{}}}";

struct MockNpmRegistry {
	port: u16,
	thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for MockNpmRegistry {
	fn drop(&mut self) {
		if let Some(thread) = self.thread.take() {
			thread
				.join()
				.unwrap_or_else(|_| panic!("mock npm registry thread panicked"));
		}
	}
}

fn mock_npm_registry_with_version_1_0_0(request_count: usize) -> MockNpmRegistry {
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock npm registry: {error}"));
	let port = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("mock npm registry address: {error}"))
		.port();
	let thread = std::thread::spawn(move || {
		for _ in 0..request_count {
			let Ok((mut stream, _)) = listener.accept() else {
				break;
			};
			let mut request = [0_u8; 2048];
			std::io::Read::read(&mut stream, &mut request)
				.unwrap_or_else(|error| panic!("read mock registry request: {error}"));
			stream
				.write_all(DUPLICATE_VERSION_RESPONSE)
				.unwrap_or_else(|error| panic!("write mock registry response: {error}"));
		}
	});
	MockNpmRegistry {
		port,
		thread: Some(thread),
	}
}

fn workspace_root() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/cli-output/publish-duplicate-version"),
		tempdir.path(),
	);
	tempdir
}

fn publish_packages_command(root: &Path, registry_port: u16, package: &str) -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.current_dir(root);
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command.env("MONOCHANGE_NO_PROGRESS", "1");
	// GitHub Actions runner environment must not leak into the trusted
	// publishing snapshot; `GITHUB_REPOSITORY` and `GITHUB_WORKFLOW_REF` would
	// otherwise flip the `tolerant` outcome from `manual_action_required` to
	// `configured` on CI while local runs commit the manual variant.
	for env_var in [
		"GITHUB_ACTIONS",
		"GITHUB_REPOSITORY",
		"GITHUB_WORKFLOW",
		"GITHUB_WORKFLOW_REF",
		"GITHUB_ENVIRONMENT",
		"GITHUB_JOB",
		"GITHUB_RUN_ID",
		"GITHUB_REF_NAME",
		"MONOCHANGE_TRUSTED_PUBLISHING_ENVIRONMENT",
	] {
		command.env_remove(env_var);
	}
	command.env(
		"MONOCHANGE_NPM_REGISTRY_URL",
		format!("http://127.0.0.1:{registry_port}"),
	);
	command.arg("step");
	command.arg("publish-packages");
	command.arg("--all");
	command.arg("--package").arg(package);
	command.arg("--dry-run");
	command.arg("--format").arg("json");
	command
}

#[test]
fn publish_packages_skips_already_published_version_by_default() {
	let mock = mock_npm_registry_with_version_1_0_0(2);
	let workspace = workspace_root();

	let output = publish_packages_command(workspace.path(), mock.port, "tolerant")
		.output()
		.unwrap_or_else(|error| panic!("run publish-packages: {error}"));

	assert!(
		output.status.success(),
		"publish-packages should succeed for an already-published version by default\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish report json: {error}"));
	let outcomes = value["package_publish"]["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("expected package_publish.packages array"));
	let outcome = &outcomes[0];
	assert_eq!(
		outcome["status"].as_str(),
		Some("skipped_existing"),
		"expected skipped_existing outcome, got: {outcome}"
	);
	assert_eq!(outcome["version"].as_str(), Some("1.0.0"));
	assert_json_snapshot!(outcome);
}

#[test]
fn publish_packages_fails_when_fail_on_duplicate_is_enabled() {
	let mock = mock_npm_registry_with_version_1_0_0(2);
	let workspace = workspace_root();

	let output = publish_packages_command(workspace.path(), mock.port, "strict")
		.output()
		.unwrap_or_else(|error| panic!("run publish-packages: {error}"));

	assert!(
		!output.status.success(),
		"publish-packages should fail for an already-published version with fail_on_duplicate\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(
		stdout.trim().is_empty(),
		"failing publish-packages run should not emit a report on stdout, got: {stdout}"
	);
	assert_snapshot!(String::from_utf8_lossy(&output.stderr).trim());
}
