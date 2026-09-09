use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

struct MockCratesIo {
	thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for MockCratesIo {
	fn drop(&mut self) {
		if let Some(thread) = self.thread.take() {
			thread
				.join()
				.unwrap_or_else(|_| panic!("mock crates.io thread panicked"));
		}
	}
}

/// Serve the crates.io API responses the readiness flow needs: the version
/// lookup must report the released version as missing, and the trusted
/// publishing probe must report the package as existing.
fn mock_crates_io(request_count: usize) -> (u16, MockCratesIo) {
	let body = "{\"crate\":{\"id\":\"fixture\",\"max_version\":\"0.1.0\"},\"versions\":[{\"num\":\"0.1.0\"}]}";
	let response = format!(
		"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
		body.len(),
		body
	);
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock crates.io: {error}"));
	let port = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("mock crates.io address: {error}"))
		.port();
	let thread = std::thread::spawn(move || {
		for _ in 0..request_count {
			let Ok((mut stream, _)) = listener.accept() else {
				break;
			};
			let mut request = [0_u8; 2048];
			std::io::Read::read(&mut stream, &mut request)
				.unwrap_or_else(|error| panic!("read mock crates.io request: {error}"));
			stream
				.write_all(response.as_bytes())
				.unwrap_or_else(|error| panic!("write mock crates.io response: {error}"));
		}
	});
	(
		port,
		MockCratesIo {
			thread: Some(thread),
		},
	)
}

fn setup_publish_readiness_repo() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("publish-readiness"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn monochange_command() -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command.env("MONOCHANGE_NO_PROGRESS", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-07");
	command
}

/// Run a command to completion with a hard timeout. On timeout the child is
/// killed and the panic names the phase and dumps the captured stderr tail,
/// so a hung subprocess becomes an actionable failure instead of stalling
/// until the nextest test timeout.
fn guarded_output(mut command: Command, phase: &str) -> std::process::Output {
	// Pipe every stream (like `Command::output`) so reader threads always
	// exist; inheriting stdio would leave the recv calls below without a
	// sender and deadlock the guard.
	command
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped());
	let mut child = command
		.spawn()
		.unwrap_or_else(|error| panic!("spawn {phase}: {error}"));
	let stdout_pipe = child.stdout.take();
	let stderr_pipe = child.stderr.take();
	let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<Vec<u8>>();
	let (stderr_tx, stderr_rx) = std::sync::mpsc::channel::<String>();
	if let Some(mut pipe) = stdout_pipe {
		std::thread::spawn(move || {
			let mut buffer = Vec::new();
			if std::io::Read::read_to_end(&mut pipe, &mut buffer).is_ok() {
				let _ = stdout_tx.send(buffer);
			}
		});
	}
	if let Some(mut pipe) = stderr_pipe {
		std::thread::spawn(move || {
			let mut buffer = String::new();
			if std::io::Read::read_to_string(&mut pipe, &mut buffer).is_ok() {
				let _ = stderr_tx.send(buffer);
			}
		});
	}
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
	let mut last_report = std::time::Instant::now() - std::time::Duration::from_secs(2);
	loop {
		if last_report.elapsed() > std::time::Duration::from_secs(1) {
			eprintln!(
				"[readiness-test] still waiting on {phase}, child pid {:?}",
				child.id()
			);
			last_report = std::time::Instant::now();
		}
		match child.try_wait() {
			Ok(Some(status)) => {
				let stdout = stdout_rx
					.recv_timeout(std::time::Duration::from_secs(10))
					.unwrap_or_default();
				let stderr = stderr_rx
					.recv_timeout(std::time::Duration::from_secs(10))
					.unwrap_or_default();
				return std::process::Output {
					status,
					stdout,
					stderr: stderr.into_bytes(),
				};
			}
			Ok(None) => {
				if std::time::Instant::now() > deadline {
					let _ = child.kill();
					let stderr_tail = stderr_rx
						.recv_timeout(std::time::Duration::from_secs(5))
						.unwrap_or_default();
					let tail: String = stderr_tail
						.chars()
						.rev()
						.take(4000)
						.collect::<Vec<_>>()
						.into_iter()
						.rev()
						.collect();
					panic!("{phase} did not finish within 90s; stderr tail:\n{tail}");
				}
				std::thread::sleep(std::time::Duration::from_millis(50));
			}
			Err(error) => panic!("wait {phase}: {error}"),
		}
	}
}

fn run_readiness_release(root: &Path) {
	eprintln!("[readiness-test] starting readiness-release");
	let mut command = monochange_command();
	command
		.current_dir(root)
		.arg("run")
		.arg("readiness-release");
	let output = guarded_output(command, "readiness-release");
	eprintln!("[readiness-test] readiness-release finished");
	assert!(
		output.status.success(),
		"readiness-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn publish_readiness_command(root: &Path, crates_io_port: u16) -> Command {
	let mut command = monochange_command();
	command.current_dir(root);
	command.env(
		"MONOCHANGE_CRATES_IO_API_URL",
		format!("http://127.0.0.1:{crates_io_port}"),
	);
	command.arg("step");
	command.arg("publish-readiness");
	command.arg("--from");
	command.arg("HEAD");
	command.arg("--format");
	command.arg("json");
	command
}

#[test]
fn publish_readiness_reports_trusted_publishing_and_publish_order() {
	// Two requests per package: the dry-run publish version lookup and the
	// trusted-publishing package-existence probe. The mock reports the
	// packages as existing so the report is ready with manual-verification
	// trusted-publishing findings in any environment (local or CI).
	let (port, _mock) = mock_crates_io(4);
	let workspace = setup_publish_readiness_repo();
	eprintln!("[readiness-test] fixture ready");
	run_readiness_release(workspace.path());
	eprintln!("[readiness-test] readiness-release committed");

	let output = guarded_output(
		publish_readiness_command(workspace.path(), port),
		"publish-readiness",
	);
	eprintln!(
		"[readiness-test] publish-readiness exited with {}",
		output.status.code().unwrap_or(-1)
	);
	assert!(
		output.status.success(),
		"publish-readiness failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish readiness json: {error}"));
	let report = &value;

	assert_eq!(
		report["status"].as_str(),
		Some("ready"),
		"unexpected readiness report: {report:#}"
	);
	assert_eq!(report["schema_version"].as_u64(), Some(3));

	let publish_order = report["publish_order"]
		.as_array()
		.unwrap_or_else(|| panic!("expected publish_order array"));
	let publish_order: Vec<String> = publish_order
		.iter()
		.map(|package| {
			package
				.as_str()
				.unwrap_or_else(|| panic!("publish order entry should be a string"))
				.to_string()
		})
		.collect();
	assert_eq!(
		publish_order,
		vec!["auth_policy".to_string(), "ledger_types".to_string()]
	);

	let packages = report["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("expected packages array"));
	assert_eq!(packages.len(), 2);
	for package in packages {
		let trusted = &package["trusted_publishing"];
		assert_eq!(
			trusted["status"].as_str(),
			Some("manual_verification_required"),
			"local runs degrade identity checks: {trusted}"
		);
		assert!(
			trusted["message"]
				.as_str()
				.unwrap_or_default()
				.contains("crates.io/crates/"),
			"expected the crates.io setup URL in {trusted}"
		);
	}

	// Empty findings are omitted from the artifact.
	let order_findings = report["order_findings"].as_array();
	assert!(
		order_findings
			.into_iter()
			.flatten()
			.all(|finding| finding["blocking"] != Value::Bool(true)),
		"dependency-corrected order must satisfy the workspace graph: {order_findings:?}"
	);
}

fn mock_crates_io_missing(request_count: usize) -> (u16, MockCratesIo) {
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock crates.io: {error}"));
	let port = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("mock crates.io address: {error}"))
		.port();
	listener
		.set_nonblocking(true)
		.unwrap_or_else(|error| panic!("set nonblocking: {error}"));
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
	let thread = std::thread::spawn(move || {
		let mut served = 0_usize;
		while served < request_count && std::time::Instant::now() < deadline {
			match listener.accept() {
				Ok((mut stream, _)) => {
					stream
						.set_nonblocking(false)
						.unwrap_or_else(|error| panic!("set blocking: {error}"));
					let mut request = [0_u8; 2048];
					let _ = std::io::Read::read(&mut stream, &mut request);
					let _ = stream.write_all(
						b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
					);
					served += 1;
				}
				Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
					std::thread::sleep(std::time::Duration::from_millis(25));
				}
				Err(_) => break,
			}
		}
	});
	(
		port,
		MockCratesIo {
			thread: Some(thread),
		},
	)
}

#[test]
fn publish_readiness_blocks_never_published_packages_in_ci_trusted_publishing() {
	// In a GitHub Actions context the trust checks are strict: packages that
	// were never published cannot use trusted publishing and must be blocked
	// with placeholder-publish guidance before anything is mutated. The
	// registry mock 404s every lookup, so the packages do not exist.
	let (port, _mock) = mock_crates_io_missing(4);
	let workspace = setup_publish_readiness_repo();
	run_readiness_release(workspace.path());

	let mut command = publish_readiness_command(workspace.path(), port);
	command.env("GITHUB_ACTIONS", "true");
	command.env("GITHUB_REPOSITORY", "acme/widgets");
	command.env(
		"GITHUB_WORKFLOW_REF",
		"acme/widgets/.github/workflows/release.yml@refs/heads/main",
	);
	command.env("GITHUB_JOB", "publish");
	let output = command
		.output()
		.unwrap_or_else(|error| panic!("run publish-readiness: {error}"));
	assert!(
		output.status.success(),
		"readiness reports blocking findings without failing the step\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish readiness json: {error}"));
	let report = &value;

	assert_eq!(report["status"].as_str(), Some("blocked"));
	let packages = report["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("expected packages array"));
	assert_eq!(packages.len(), 2);
	for package in packages {
		assert_eq!(package["status"].as_str(), Some("blocked"));
		let trusted = &package["trusted_publishing"];
		assert_eq!(trusted["status"].as_str(), Some("blocked"));
		let message = trusted["message"].as_str().unwrap_or_default();
		assert!(
			message.contains("placeholder-publish"),
			"expected bootstrap guidance in {message}"
		);
	}
}
