//! Integration coverage for released-issue comments and closure.
//!
//! These tests spawn the real monochange binary against a workspace whose
//! committed release record carries the issue references captured from a
//! release pull request body: `#7` and `#8` come from one comma-separated
//! `Closes #7, #8` line (GitHub only auto-closes the first issue of such a
//! list), and `#9` is mentioned without any closing keyword. A loopback mock
//! of the GitHub REST API records every request, so the tests assert — end to
//! end, with no network access — that `comment-released-issues` posts the
//! release comment on every linked issue, closes only the closing-keyword
//! issues when `--auto-close-issues` is passed, and patches no issue state at
//! all without the flag.

#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]

use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use tempfile::TempDir;

const OWNER: &str = "ifiokjr";
const REPO: &str = "monochange";
const ISSUE_NUMBERS: [u64; 3] = [7, 8, 9];

/// One captured HTTP request as its `METHOD /path` request line.
struct CapturedRequest {
	method: String,
	path: String,
}

impl CapturedRequest {
	fn request_line(&self) -> String {
		format!("{} {}", self.method, self.path)
	}
}

struct MockGithubServer {
	base_url: String,
	requests: Arc<Mutex<Vec<CapturedRequest>>>,
	stop: Arc<AtomicBool>,
}

impl MockGithubServer {
	fn request_count(&self, request_line: &str) -> usize {
		self.requests
			.lock()
			.unwrap()
			.iter()
			.filter(|request| request.request_line() == request_line)
			.count()
	}
}

impl Drop for MockGithubServer {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Relaxed);
	}
}

fn github_api_response(method: &str, path: &str) -> String {
	let issue_comment_route = format!("/repos/{OWNER}/{REPO}/issues/");
	let Some(rest) = path.strip_prefix(&issue_comment_route) else {
		return not_found_response();
	};
	let (number, tail) = match rest.split_once('/') {
		Some((number, tail)) => (number, Some(tail)),
		None => (rest, None),
	};
	let html_url = format!("https://github.com/{OWNER}/{REPO}/issues/{number}#comment-1");
	match (method, tail) {
		("GET", Some("comments")) => json_response(200, "[]"),
		("POST", Some("comments")) => {
			json_response(201, &format!(r#"{{"html_url":"{html_url}"}}"#))
		}
		("PATCH", None) => json_response(200, r#"{"state":"closed"}"#),
		_ => not_found_response(),
	}
}

fn json_response(status: u16, body: &str) -> String {
	format!(
		"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
		body.len()
	)
}

fn not_found_response() -> String {
	"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
}

fn spawn_mock_github_server() -> MockGithubServer {
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock GitHub server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("mock GitHub server address: {error}"))
		.to_string();
	let requests = Arc::new(Mutex::new(Vec::new()));
	let captured = Arc::clone(&requests);
	let stop = Arc::new(AtomicBool::new(false));
	let stop_for_thread = Arc::clone(&stop);
	let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(0);
	std::thread::spawn(move || {
		let _ = listener.set_nonblocking(true);
		ready_tx
			.send(())
			.unwrap_or_else(|error| panic!("signal mock GitHub server readiness: {error}"));
		while !stop_for_thread.load(Ordering::Relaxed) {
			let Ok((mut stream, _)) = listener.accept() else {
				std::thread::sleep(Duration::from_millis(5));
				continue;
			};
			// The listener is nonblocking so shutdown can be polled. Each accepted
			// connection must use blocking I/O for the complete request/response.
			let _ = stream.set_nonblocking(false);
			let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
			let head = read_until_header_end(&mut stream);
			let body_length = content_length(&head);
			let _ = read_exact_bytes(&mut stream, body_length);
			let request_line = head
				.lines()
				.next()
				.unwrap_or_default()
				.split(" HTTP/")
				.next()
				.unwrap_or_default()
				.to_string();
			let (method, path) = match request_line.split_once(' ') {
				Some((method, path)) => (method.to_string(), path.to_string()),
				None => (String::new(), String::new()),
			};
			let response = github_api_response(&method, &path);
			captured
				.lock()
				.unwrap()
				.push(CapturedRequest { method, path });
			let _ = stream.write_all(response.as_bytes());
			let _ = stream.flush();
		}
	});
	ready_rx
		.recv()
		.unwrap_or_else(|error| panic!("wait for mock GitHub server readiness: {error}"));
	MockGithubServer {
		base_url: format!("http://{address}"),
		requests,
		stop,
	}
}

fn read_until_header_end(stream: &mut std::net::TcpStream) -> String {
	let mut head = Vec::new();
	let mut buffer = [0_u8; 4096];
	loop {
		match stream.read(&mut buffer) {
			Ok(0) | Err(_) => break,
			Ok(read) => {
				head.extend_from_slice(&buffer[..read]);
				if head.windows(4).any(|window| window == b"\r\n\r\n") {
					break;
				}
			}
		}
	}
	String::from_utf8_lossy(&head).into_owned()
}

fn content_length(head: &str) -> usize {
	head.lines()
		.find_map(|line| {
			let (name, value) = line.split_once(':')?;
			name.trim()
				.eq_ignore_ascii_case("content-length")
				.then(|| value.trim().parse::<usize>().ok())?
		})
		.unwrap_or(0)
}

fn read_exact_bytes(stream: &mut std::net::TcpStream, length: usize) -> Vec<u8> {
	let mut body = vec![0_u8; length];
	if length > 0 {
		let _ = stream.read_exact(&mut body);
	}
	body
}

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_workspace() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(
		&fixture_path("comment-released-issues/close-keyword-issues/workspace"),
		root,
	);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn run_comment_released_issues(
	root: &Path,
	base_url: &str,
	extra_args: &[&str],
) -> std::process::Output {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		// Ambient GitHub configuration must not leak into the run.
		.env_remove("GITHUB_ACTIONS")
		.env_remove("GH_TOKEN")
		.env("GITHUB_TOKEN", "test-token")
		.env("GITHUB_API_URL", base_url)
		.arg("step")
		.arg("comment-released-issues")
		.arg("--from-ref")
		.arg("HEAD")
		.arg("--format")
		.arg("json")
		.args(extra_args);
	command
		.output()
		.unwrap_or_else(|error| panic!("run comment-released-issues: {error}"))
}

fn patch_request_line(number: u64) -> String {
	format!("PATCH /repos/{OWNER}/{REPO}/issues/{number}")
}

fn comment_request_line(number: u64) -> String {
	format!("POST /repos/{OWNER}/{REPO}/issues/{number}/comments")
}

fn captured_request_lines(server: &MockGithubServer) -> String {
	server
		.requests
		.lock()
		.unwrap()
		.iter()
		.map(|request| request.request_line())
		.collect::<Vec<_>>()
		.join("\n")
}

#[test]
fn auto_close_closes_closing_keyword_issues_and_keeps_plain_mentions_open() {
	let server = spawn_mock_github_server();
	let tempdir = setup_workspace();
	let output =
		run_comment_released_issues(tempdir.path(), &server.base_url, &["--auto-close-issues"]);
	assert!(
		output.status.success(),
		"comment-released-issues failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	// The release comment is posted on every linked issue.
	for number in ISSUE_NUMBERS {
		assert_eq!(
			server.request_count(&comment_request_line(number)),
			1,
			"expected exactly one release comment on issue #{number}"
		);
	}
	// Closing-keyword issues are closed — including the comma-list entry
	// GitHub never auto-closed at merge time.
	assert_eq!(
		server.request_count(&patch_request_line(7)),
		1,
		"issue #7 must be closed"
	);
	assert_eq!(
		server.request_count(&patch_request_line(8)),
		1,
		"comma-list issue #8 must be closed even though GitHub did not close it at merge time"
	);
	// A plain mention must stay open.
	assert_eq!(
		server.request_count(&patch_request_line(9)),
		0,
		"issue #9 is mentioned without a closing keyword and must stay open; captured:\n{}",
		captured_request_lines(&server)
	);

	assert_snapshot!(String::from_utf8_lossy(&output.stdout));
}

#[test]
fn without_auto_close_no_issue_state_is_patched() {
	let server = spawn_mock_github_server();
	let tempdir = setup_workspace();
	let output = run_comment_released_issues(tempdir.path(), &server.base_url, &[]);
	assert!(
		output.status.success(),
		"comment-released-issues failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	for number in ISSUE_NUMBERS {
		assert_eq!(
			server.request_count(&comment_request_line(number)),
			1,
			"expected exactly one release comment on issue #{number}"
		);
		assert_eq!(
			server.request_count(&patch_request_line(number)),
			0,
			"issue #{number} must not be patched without --auto-close-issues; captured:\n{}",
			captured_request_lines(&server)
		);
	}

	assert_snapshot!(String::from_utf8_lossy(&output.stdout));
}
