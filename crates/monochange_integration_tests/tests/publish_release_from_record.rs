//! Integration coverage for provider releases published from a committed
//! release record.
//!
//! Production workflows publish the GitHub release after the release pull
//! request merges: `monochange step publish-release --from-ref HEAD` finds the
//! release record in git history and replays it. These tests reproduce that
//! path end to end against a loopback mock of the GitHub REST API — the mock
//! records every request body, so the tests assert the exact release `name`
//! and `body` monochange sends:
//!
//! - the `name` carries the tag-style version with the date for primary
//!   targets (`v1.1.0 (2026-04-06)`) and names the release owner for
//!   namespaced targets (`core v1.1.0 (2026-04-06)`) — never the bare tag —
//!   both for current records (persisted titles) and for v0.8-era records
//!   that predate persisted titles (synthesized from the record's
//!   `created_at`);
//! - the `body` drops the changelog's version title, keeps the group summary,
//!   and promotes sections to `##` with expanded entries as `###`.

#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]

use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

const OWNER: &str = "ifiokjr";
const REPO: &str = "monochange";

/// One captured HTTP request: request line plus the JSON request body.
#[derive(Clone, Debug)]
struct CapturedRequest {
	method: String,
	path: String,
	body: String,
}

struct MockGithubServer {
	base_url: String,
	requests: Arc<Mutex<Vec<CapturedRequest>>>,
	stop: Arc<std::sync::atomic::AtomicBool>,
}

impl MockGithubServer {
	fn post_bodies_under(&self, path_prefix: &str) -> Vec<Value> {
		self.requests
			.lock()
			.unwrap()
			.iter()
			.filter(|request| {
				request.method == "POST"
					&& request.path.starts_with(path_prefix)
					&& !request.body.trim().is_empty()
			})
			.map(|request| {
				serde_json::from_str::<Value>(&request.body).unwrap_or_else(|error| {
					panic!("parse captured body: {error}\n{}", request.body)
				})
			})
			.collect()
	}
}

impl Drop for MockGithubServer {
	fn drop(&mut self) {
		self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
	}
}

/// Minimal GitHub REST responder for release publishing: the tag lookup misses
/// (404) so the release is created with POST, and the create succeeds.
fn github_api_response(method: &str, path: &str) -> String {
	let not_found = not_found_response();
	let releases_route = format!("/repos/{OWNER}/{REPO}/releases");
	if method == "GET" && path.starts_with(&format!("{releases_route}/tags/")) {
		return not_found;
	}
	if method == "POST" && path == releases_route {
		let body =
			r#"{"id":1,"html_url":"https://github.com/ifiokjr/monochange/releases/tag/v1.1.0"}"#;
		return format!(
			"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
			body.len()
		);
	}
	not_found
}

fn not_found_response() -> String {
	let body = r#"{"message":"Not Found","documentation_url":"https://docs.github.com/rest"}"#;
	format!(
		"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
		body.len()
	)
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
	let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let stop_for_thread = Arc::clone(&stop);
	let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(0);
	std::thread::spawn(move || {
		listener
			.set_nonblocking(true)
			.expect("set mock listener nonblocking");
		ready_tx
			.send(())
			.unwrap_or_else(|error| panic!("signal mock GitHub server readiness: {error}"));
		while !stop_for_thread.load(std::sync::atomic::Ordering::Relaxed) {
			let Ok((stream, _)) = listener.accept() else {
				std::thread::sleep(Duration::from_millis(5));
				continue;
			};
			let captured = Arc::clone(&captured);
			std::thread::spawn(move || handle_request(stream, captured));
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

fn handle_request(mut stream: std::net::TcpStream, captured: Arc<Mutex<Vec<CapturedRequest>>>) {
	// macOS inherits the listener's nonblocking mode on accepted sockets.
	// Wait for the headers instead of treating an initial WouldBlock as EOF.
	stream
		.set_nonblocking(false)
		.expect("set mock connection blocking");
	stream
		.set_read_timeout(Some(Duration::from_secs(5)))
		.expect("set mock connection read timeout");
	let (head, leftover) = read_until_header_end(&mut stream);
	if head.is_empty() {
		return;
	}
	let body = if head.lines().any(|line| {
		line.to_ascii_lowercase()
			.starts_with("transfer-encoding: chunked")
	}) {
		read_chunked_body(&mut stream, leftover)
	} else {
		let remaining = content_length(&head).saturating_sub(leftover.len());
		let mut body = leftover;
		let mut pending = Vec::new();
		body.extend_from_slice(&read_exact_bytes(&mut stream, remaining, &mut pending));
		body
	};
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
	captured.lock().unwrap().push(CapturedRequest {
		method,
		path,
		body: String::from_utf8_lossy(&body).into_owned(),
	});
	let _ = stream.write_all(response.as_bytes());
	let _ = stream.flush();
}

/// Read until the end of the HTTP header block, returning the header text and
/// any request-body bytes that arrived in the same read.
fn read_until_header_end(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
	let mut buffer = Vec::new();
	let mut chunk = [0_u8; 4096];
	loop {
		match stream.read(&mut chunk) {
			Ok(0) | Err(_) => break,
			Ok(read) => {
				buffer.extend_from_slice(&chunk[..read]);
				if let Some(position) = find_header_end(&buffer) {
					let leftover = buffer.split_off(position + 4);
					buffer.truncate(position + 4);
					return (String::from_utf8_lossy(&buffer).into_owned(), leftover);
				}
			}
		}
	}
	(String::from_utf8_lossy(&buffer).into_owned(), Vec::new())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
	buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Decode a chunked request body: repeated `<size-hex>\r\n<data>\r\n>` frames
/// terminated by a zero-sized chunk.
fn read_chunked_body(stream: &mut std::net::TcpStream, mut leftover: Vec<u8>) -> Vec<u8> {
	let mut body = Vec::new();
	let mut pending = Vec::new();
	pending.append(&mut leftover);
	loop {
		let size_line = read_line(stream, &mut pending);
		let Ok(size) = usize::from_str_radix(size_line.trim(), 16) else {
			break;
		};
		if size == 0 {
			// Consume the trailing CRLF after the terminal chunk.
			let _ = read_line(stream, &mut pending);
			break;
		}
		body.extend_from_slice(&read_exact_bytes(stream, size, &mut pending));
		// Consume the CRLF that follows each chunk's data.
		let _ = read_line(stream, &mut pending);
	}
	body
}

fn read_line(stream: &mut std::net::TcpStream, pending: &mut Vec<u8>) -> String {
	let mut line = Vec::new();
	loop {
		while pending.is_empty() {
			let mut byte = [0_u8; 1];
			match stream.read(&mut byte) {
				Ok(0) | Err(_) => return String::from_utf8_lossy(&line).into_owned(),
				Ok(_) => pending.push(byte[0]),
			}
		}
		line.push(pending.remove(0));
		if line.ends_with(b"\r\n") {
			break;
		}
	}
	String::from_utf8_lossy(&line).into_owned()
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

fn read_exact_bytes(
	stream: &mut std::net::TcpStream,
	length: usize,
	pending: &mut Vec<u8>,
) -> Vec<u8> {
	let from_pending = pending.len().min(length);
	let mut body = pending.drain(..from_pending).collect::<Vec<_>>();
	let mut remaining = length - from_pending;
	let mut chunk = vec![0_u8; remaining.max(1)];
	while remaining > 0 {
		match stream.read(&mut chunk[..remaining]) {
			Ok(0) | Err(_) => break,
			Ok(read) => {
				body.extend_from_slice(&chunk[..read]);
				remaining -= read;
			}
		}
	}
	body
}

fn fixture_path(scenario: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/github-releases")
		.join(scenario)
}

fn setup_workspace(scenario: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(scenario), root);
	if scenario == "group" {
		// The fixture's changeset renders as a compact bullet; a second
		// changeset with multiline details exercises the expanded entry style,
		// whose heading is what provider bodies promote from `####` to `###`.
		std::fs::write(
			root.join(".changeset/expanded-feature.md"),
			"---\ncore: minor\n---\n\n# Add expanded feature\n\nDetails paragraph one.\n\nDetails paragraph two with a migration note.\n",
		)
		.unwrap_or_else(|error| panic!("write expanded changeset: {error}"));
	}
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	let output = Command::new("git")
		.current_dir(root)
		.env("GIT_AUTHOR_DATE", "2026-04-05T00:00:00Z")
		.env("GIT_COMMITTER_DATE", "2026-04-05T00:00:00Z")
		.args(["commit", "-m", "initial"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));
	assert!(
		output.status.success(),
		"git commit failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	tempdir
}

fn run_monochange(root: &Path, args: &[&str]) -> std::process::Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {}: {error}", args.join(" ")))
}

/// Write the release record the way the release pull request commit would: the
/// prepared release is recorded, the consumed changesets are deleted, and the
/// result is committed on `main`.
fn commit_release_record(root: &Path) -> PathBuf {
	let output = run_monochange(
		root,
		&[
			"step",
			"prepare-release",
			"--dry-run",
			"--release-json",
			"--format",
			"json",
		],
	);
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	let changeset_dir = root.join(".changeset");
	if changeset_dir.is_dir() {
		for entry in std::fs::read_dir(&changeset_dir)
			.unwrap_or_else(|error| panic!("read changeset dir: {error}"))
		{
			let path = entry
				.unwrap_or_else(|error| panic!("read changeset entry: {error}"))
				.path();
			if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
				std::fs::remove_file(&path)
					.unwrap_or_else(|error| panic!("remove consumed changeset: {error}"));
			}
		}
	}

	git(root, &["add", "."]);
	git(root, &["commit", "-m", "chore(release): prepare release"]);
	find_release_record(root)
}

fn find_release_record(root: &Path) -> PathBuf {
	let releases_dir = root.join(".monochange/releases");
	let mut paths = std::fs::read_dir(&releases_dir)
		.unwrap_or_else(|error| panic!("read releases dir: {error}"))
		.map(|entry| {
			entry
				.unwrap_or_else(|error| panic!("read release entry: {error}"))
				.path()
				.join("release.json")
		})
		.filter(|path| path.is_file())
		.collect::<Vec<_>>();
	paths.sort();
	paths
		.into_iter()
		.next()
		.unwrap_or_else(|| panic!("expected a committed release record"))
}

fn assert_loopback_destination(base_url: &str) {
	let address = base_url
		.strip_prefix("http://")
		.and_then(|address| address.parse::<SocketAddr>().ok())
		.expect("mock GitHub URL must be an explicit HTTP socket address");
	assert!(
		address.ip().is_loopback() && address.port() != 0,
		"release replay tests must use a bound loopback server"
	);
}

#[test]
fn release_replay_rejects_non_mock_destinations() {
	for base_url in [
		"https://api.github.com",
		"http://192.0.2.1:1234",
		"http://127.0.0.1:0",
		"http://localhost:1234",
		"http://127.0.0.1:1234/path",
	] {
		assert!(
			std::panic::catch_unwind(|| assert_loopback_destination(base_url)).is_err(),
			"unexpectedly accepted {base_url}"
		);
	}
}

#[test]
fn mock_github_waits_for_delayed_request_headers() {
	let server = spawn_mock_github_server();
	assert_loopback_destination(&server.base_url);
	let address = server.base_url.strip_prefix("http://").expect("HTTP URL");
	let mut stream = std::net::TcpStream::connect(address).expect("connect to mock GitHub");
	stream
		.set_read_timeout(Some(Duration::from_secs(5)))
		.expect("set client read timeout");
	std::thread::sleep(Duration::from_millis(100));
	stream
		.write_all(
			b"POST /repos/ifiokjr/monochange/releases HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}",
		)
		.expect("write delayed mock request");
	let mut response = String::new();
	stream
		.read_to_string(&mut response)
		.expect("read mock response");
	assert!(response.starts_with("HTTP/1.1 201 Created\r\n"));
	assert_eq!(
		server.post_bodies_under("/repos/ifiokjr/monochange/releases"),
		vec![serde_json::json!({})]
	);
}

fn publish_release_from_record(root: &Path, server: &MockGithubServer) {
	assert_loopback_destination(&server.base_url);
	let configuration = monochange_config::load_workspace_configuration(root)
		.expect("load mock release configuration");
	let source = configuration.source.as_ref().expect("GitHub source");
	assert!(
		source
			.api_url
			.as_deref()
			.is_none_or(|api_url| api_url == server.base_url),
		"fixture API URL must not override the loopback mock destination"
	);
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		// Ambient GitHub configuration must not leak into the run.
		.env_remove("GITHUB_ACTIONS")
		.env_remove("GH_TOKEN")
		.env("GITHUB_TOKEN", "test-token")
		.env("GITHUB_API_URL", &server.base_url)
		.args([
			"step",
			"publish-release",
			"--from-ref",
			"HEAD",
			"--format",
			"json",
		])
		.output()
		.unwrap_or_else(|error| panic!("run publish-release: {error}"));
	assert!(
		output.status.success(),
		"publish-release failed against {}\nstdout:\n{}\nstderr:\n{}\ncaptured requests:\n{:#?}",
		server.base_url,
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
		server.requests.lock().unwrap()
	);
}

struct PublishedRelease {
	name: String,
	body: String,
	tag_name: String,
}

fn published_release_for_tag(server: &MockGithubServer, tag_name: &str) -> PublishedRelease {
	let bodies = server.post_bodies_under(&format!("/repos/{OWNER}/{REPO}/releases"));
	let payload = bodies
		.iter()
		.find(|payload| payload["tag_name"].as_str() == Some(tag_name))
		.unwrap_or_else(|| {
			let captured = server
				.requests
				.lock()
				.unwrap()
				.iter()
				.map(|request| {
					format!(
						"{} {} ({} bytes): {}",
						request.method,
						request.path,
						request.body.len(),
						&request.body[..request.body.len().min(300)]
					)
				})
				.collect::<Vec<_>>()
				.join("\n");
			panic!("expected a release create for {tag_name}, captured requests:\n{captured}")
		});
	PublishedRelease {
		name: payload["name"]
			.as_str()
			.unwrap_or_else(|| panic!("release name was not a string: {payload}"))
			.to_string(),
		body: payload["body"]
			.as_str()
			.unwrap_or_else(|| panic!("release body was not a string: {payload}"))
			.to_string(),
		tag_name: payload["tag_name"]
			.as_str()
			.unwrap_or_else(|| panic!("release tag was not a string: {payload}"))
			.to_string(),
	}
}

fn normalize_commit_links(contents: &str) -> String {
	contents
		.lines()
		.map(|line| {
			if line.contains("_Introduced in:_ [`") {
				"_Owner:_ test · _Introduced in:_ [`[commit]`](https://github.com/ifiokjr/monochange/commit/[commit])".to_string()
			} else {
				line.to_string()
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

/// Assertions shared by every provider release body: the body is attached to
/// the tag, so it drops the changelog's linked version title and promotes
/// sections to `##`.
fn assert_promoted_body(release: &PublishedRelease) {
	assert!(
		!release.body.contains("## [1.1.0]"),
		"body should not repeat the version title:\n{}",
		release.body
	);
	assert!(
		release.body.contains("## "),
		"sections should render as h2:\n{}",
		release.body
	);
	assert_ne!(release.name, release.tag_name);
}

fn assert_release_notes_shape(release: &PublishedRelease) {
	// A primary release axis has one version line, so the name carries the
	// tag-style version with the date — never the bare tag.
	assert_eq!(
		release.name, "v1.1.0 (2026-04-06)",
		"primary release name should carry the tag-style version and date"
	);
	assert_promoted_body(release);
	assert!(
		release.body.starts_with("Grouped release for `sdk`."),
		"grouped body should open with the summary:\n{}",
		release.body
	);
	assert!(
		release.body.lines().any(|line| line.starts_with("### ")),
		"expanded entries should render as h3:\n{}",
		release.body
	);
}

#[test]
fn publish_release_from_record_names_owner_and_promotes_body_headings() {
	let tempdir = setup_workspace("group");
	let root = tempdir.path();
	commit_release_record(root);

	let server = spawn_mock_github_server();
	publish_release_from_record(root, &server);

	let release = published_release_for_tag(&server, "v1.1.0");
	assert_release_notes_shape(&release);
	assert_snapshot!("record_replay__name", release.name);
	assert_snapshot!("record_replay__body", normalize_commit_links(&release.body));
}

#[test]
fn publish_release_from_legacy_record_synthesizes_format_default_title() {
	let tempdir = setup_workspace("group");
	let root = tempdir.path();
	let record_path = commit_release_record(root);

	// A v0.8-era record predates persisted titles; strip them and commit the
	// record again the way git history would hold it.
	let record: Value = serde_json::from_str(
		&std::fs::read_to_string(&record_path)
			.unwrap_or_else(|error| panic!("read release record: {error}")),
	)
	.unwrap_or_else(|error| panic!("parse release record: {error}"));
	let mut legacy = record.clone();
	for target in legacy["release_targets"]
		.as_array_mut()
		.unwrap_or_else(|| panic!("expected release targets"))
	{
		target["rendered_title"].take();
		target["rendered_changelog_title"].take();
	}
	assert_ne!(legacy, record, "expected the record to persist titles");
	std::fs::write(&record_path, serde_json::to_string_pretty(&legacy).unwrap())
		.unwrap_or_else(|error| panic!("write legacy record: {error}"));
	git(root, &["add", "."]);
	git(
		root,
		&["commit", "-m", "chore(release): strip persisted titles"],
	);

	let server = spawn_mock_github_server();
	publish_release_from_record(root, &server);

	let release = published_release_for_tag(&server, "v1.1.0");
	assert_release_notes_shape(&release);
	assert_snapshot!("legacy_record__name", release.name);
}

#[test]
fn publish_release_from_record_names_owner_for_namespaced_targets() {
	let tempdir = setup_workspace("ungrouped");
	let root = tempdir.path();
	commit_release_record(root);

	let server = spawn_mock_github_server();
	publish_release_from_record(root, &server);

	// Namespaced workspaces release several axes at once, so the title names
	// the package the release belongs to.
	let release = published_release_for_tag(&server, "core/v1.1.0");
	assert_eq!(
		release.name, "core v1.1.0 (2026-04-06)",
		"namespaced release name should name the release owner"
	);
	assert_promoted_body(&release);
	assert_snapshot!("namespaced_record__name", release.name);
	assert_snapshot!(
		"namespaced_record__body",
		normalize_commit_links(&release.body)
	);
}
