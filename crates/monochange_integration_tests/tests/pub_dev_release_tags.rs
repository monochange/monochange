//! Integration coverage for publishing pub.dev packages from their own
//! release tag.
//!
//! pub.dev trusted publishing only accepts an upload from a GitHub Actions run
//! whose event is `push` or `workflow_dispatch` and whose ref is the
//! package's release tag. Each fixture releases a different mix of packages
//! and tags; the tests run the real binary with the `GITHUB_*` variables
//! GitHub Actions sets, against registry mocks, and check which packages each
//! run publishes.

#![allow(clippy::disallowed_methods)]

use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use httpmock::Method::GET;
use httpmock::Mock;
use httpmock::MockServer;
use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use rstest::rstest;
use serde_json::Value;
use tempfile::TempDir;

const FIXTURE_ROOT: &str = "../../fixtures/tests/pub-dev-release-tags";
const REPOSITORY: &str = "example/pub-dev-release-tags";

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join(FIXTURE_ROOT)
		.join(relative)
}

fn monochange_command(root: &Path) -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-11-25")
		.env_remove("RUST_LOG");

	// The test process may itself run inside GitHub Actions; every run below
	// sets the variables it needs.
	for name in [
		"CI",
		"GITHUB_ACTIONS",
		"GITHUB_EVENT_NAME",
		"GITHUB_REF",
		"GITHUB_REF_NAME",
		"GITHUB_REF_TYPE",
		"GITHUB_REPOSITORY",
		"GITHUB_WORKFLOW",
		"GITHUB_WORKFLOW_REF",
		"GITHUB_ENVIRONMENT",
		"GITLAB_CI",
		"CIRCLECI",
		"PUB_TOKEN",
		"ACTIONS_ID_TOKEN_REQUEST_URL",
		"ACTIONS_ID_TOKEN_REQUEST_TOKEN",
	] {
		command.env_remove(name);
	}

	command
}

fn stdout_of(output: &Output) -> String {
	String::from_utf8(output.stdout.clone())
		.unwrap_or_else(|error| panic!("stdout was not UTF-8: {error}"))
}

fn stderr_of(output: &Output) -> String {
	String::from_utf8(output.stderr.clone())
		.unwrap_or_else(|error| panic!("stderr was not UTF-8: {error}"))
}

fn assert_success(output: &Output, what: &str) {
	assert!(
		output.status.success(),
		"{what} failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(output),
		stderr_of(output)
	);
}

/// Copy a workspace fixture into a git repository with one commit and no
/// release record.
fn setup_workspace(fixture: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(fixture), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "test"]);
	git(root, &["config", "user.email", "test@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	tempdir
}

/// Copy a workspace fixture and create its release commit, so the publish
/// step finds the release record at `HEAD`.
fn setup_released_workspace(fixture: &str) -> TempDir {
	let tempdir = setup_workspace(fixture);
	let root = tempdir.path();

	let output = monochange_command(root)
		.arg("commit-release")
		.output()
		.unwrap_or_else(|error| panic!("run commit-release: {error}"));
	assert_success(&output, "commit-release");

	tempdir
}

/// `PATH` with the fixture's `cargo`, `dart` and `npm` stand-ins first, so
/// planned packages run a dry-run publish command without the real
/// toolchains.
fn path_with_tool_stubs() -> OsString {
	let mut paths = vec![fixture_path("tools")];
	paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
	env::join_paths(paths).unwrap_or_else(|error| panic!("join PATH: {error}"))
}

/// The environment GitHub Actions gives one publish run.
#[derive(Debug, Clone, Copy)]
struct GitHubRun<'a> {
	/// `GITHUB_REF`; `None` leaves it unset.
	git_ref: Option<&'a str>,
	/// `GITHUB_REF_TYPE`; `None` derives it from the ref, as GitHub does.
	ref_type: Option<&'a str>,
	event: &'a str,
	repository: &'a str,
	/// `false` sets `GITHUB_WORKFLOW_REF` without `GITHUB_ACTIONS=true`.
	github_actions: bool,
	pub_token: Option<&'a str>,
}

impl<'a> GitHubRun<'a> {
	fn on(git_ref: &'a str) -> Self {
		Self {
			git_ref: Some(git_ref),
			ref_type: None,
			event: "workflow_dispatch",
			repository: REPOSITORY,
			github_actions: true,
			pub_token: None,
		}
	}

	fn event(self, event: &'a str) -> Self {
		Self { event, ..self }
	}

	fn ref_type(self, ref_type: &'a str) -> Self {
		Self {
			ref_type: Some(ref_type),
			..self
		}
	}

	fn without_ref(self) -> Self {
		Self {
			git_ref: None,
			..self
		}
	}

	fn repository(self, repository: &'a str) -> Self {
		Self { repository, ..self }
	}

	fn without_github_actions_flag(self) -> Self {
		Self {
			github_actions: false,
			..self
		}
	}

	fn pub_token(self, token: &'a str) -> Self {
		Self {
			pub_token: Some(token),
			..self
		}
	}

	fn apply(&self, command: &mut Command) {
		let workflow_ref_suffix = self.git_ref.unwrap_or("refs/heads/main");
		command
			.env("CI", "true")
			.env("GITHUB_EVENT_NAME", self.event)
			.env("GITHUB_REPOSITORY", self.repository)
			.env("GITHUB_WORKFLOW", "publish")
			.env(
				"GITHUB_WORKFLOW_REF",
				format!("{REPOSITORY}/.github/workflows/publish.yml@{workflow_ref_suffix}"),
			);

		if self.github_actions {
			command.env("GITHUB_ACTIONS", "true");
		}

		if let Some(git_ref) = self.git_ref {
			let (derived_type, name) = git_ref.strip_prefix("refs/tags/").map_or_else(
				|| {
					(
						"branch",
						git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref),
					)
				},
				|tag| ("tag", tag),
			);
			command
				.env("GITHUB_REF", git_ref)
				.env("GITHUB_REF_NAME", name)
				.env("GITHUB_REF_TYPE", self.ref_type.unwrap_or(derived_type));
		}

		if let Some(token) = self.pub_token {
			command.env("PUB_TOKEN", token);
		}
	}
}

/// Registry mocks for every package the fixtures release.
struct Registries {
	server: MockServer,
}

impl Registries {
	/// Every package is unpublished.
	fn unpublished() -> Self {
		Self {
			server: MockServer::start(),
		}
	}

	/// Mock one registry lookup, returning `status` and `body`.
	fn lookup(&self, path: &str, status: u16, body: &Value) -> Mock<'_> {
		let body = body.clone();
		self.server.mock(|when, then| {
			when.method(GET).path(path);
			then.status(status).json_body(body);
		})
	}

	/// Every lookup of every registry finds no published version, for tests
	/// that run each fixture's packages without naming them.
	fn every_package_unpublished(&self) -> Vec<Mock<'_>> {
		[
			("/npm/", serde_json::json!({ "error": "Not found" })),
			(
				"/pub/packages/",
				serde_json::json!({ "error": { "code": "NotFound" } }),
			),
			("/crates-io/crates/", serde_json::json!({ "errors": [] })),
		]
		.into_iter()
		.map(|(prefix, body)| {
			self.server.mock(|when, then| {
				when.method(GET).path_prefix(prefix);
				then.status(404).json_body(body);
			})
		})
		.collect()
	}

	fn npm(&self, package: &str) -> Mock<'_> {
		self.lookup(
			&format!("/npm/{package}"),
			404,
			&serde_json::json!({ "error": "Not found" }),
		)
	}

	fn npm_published(&self, package: &str, version: &str) -> Mock<'_> {
		self.lookup(
			&format!("/npm/{package}"),
			200,
			&serde_json::json!({ "name": package, "versions": { version: {} } }),
		)
	}

	fn pub_dev(&self, package: &str) -> Mock<'_> {
		self.lookup(
			&format!("/pub/packages/{package}"),
			404,
			&serde_json::json!({ "error": { "code": "NotFound" } }),
		)
	}

	fn pub_dev_published(&self, package: &str, version: &str) -> Mock<'_> {
		self.lookup(
			&format!("/pub/packages/{package}"),
			200,
			&serde_json::json!({ "name": package, "versions": [{ "version": version }] }),
		)
	}

	fn crates_io(&self, package: &str) -> Mock<'_> {
		self.lookup(
			&format!("/crates-io/crates/{package}"),
			404,
			&serde_json::json!({ "errors": [] }),
		)
	}

	fn apply(&self, command: &mut Command) {
		command
			.env("MONOCHANGE_NPM_REGISTRY_URL", self.server.url("/npm"))
			.env("MONOCHANGE_PUB_DEV_API_URL", self.server.url("/pub"))
			.env(
				"MONOCHANGE_CRATES_IO_API_URL",
				self.server.url("/crates-io"),
			)
			.env(
				"MONOCHANGE_CRATES_IO_INDEX_URL",
				self.server.url("/crates-io-index"),
			);
	}
}

/// Run `monochange <args>` as the GitHub Actions run `run`, against
/// `registries`.
fn run_monochange(
	root: &Path,
	registries: &Registries,
	run: GitHubRun<'_>,
	args: &[&str],
) -> Output {
	monochange_run_command(root, registries, run)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {args:?}: {error}"))
}

/// The `monochange` command for the GitHub Actions run `run`, against
/// `registries`, with the tool stand-ins on `PATH`.
fn monochange_run_command(root: &Path, registries: &Registries, run: GitHubRun<'_>) -> Command {
	let mut command = monochange_command(root);
	command.env("PATH", path_with_tool_stubs());
	registries.apply(&mut command);
	run.apply(&mut command);
	command
}

/// Run the publish dry run as the GitHub Actions run `run`.
fn publish_dry_run(
	root: &Path,
	registries: &Registries,
	run: GitHubRun<'_>,
	format: &str,
) -> Output {
	run_monochange(
		root,
		registries,
		run,
		&["publish", "packages", "--dry-run", "--format", format],
	)
}

/// Run the publish dry run in JSON and return `package_publish`.
fn publish_dry_run_json(root: &Path, registries: &Registries, run: GitHubRun<'_>) -> Value {
	let output = publish_dry_run(root, registries, run, "json");
	assert_success(&output, "publish dry run");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish json: {error}"));
	value["package_publish"].clone()
}

/// One line per package: `<package> <status>`, plus the deferral reason and
/// tag for deferred packages, sorted by package.
fn package_rows(package_publish: &Value) -> Vec<String> {
	let mut rows = package_publish["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages array: {package_publish}"))
		.iter()
		.map(|package| {
			let mut row = format!(
				"{} {}",
				package["package"].as_str().unwrap_or_default(),
				package["status"].as_str().unwrap_or_default()
			);
			let deferred_to = &package["deferred_to"];

			if !deferred_to.is_null() {
				row.push_str(&format!(
					" {} {}",
					deferred_to["reason"].as_str().unwrap_or("<no reason>"),
					deferred_to["tag"].as_str().unwrap_or_default()
				));
			}

			row
		})
		.collect::<Vec<_>>();
	rows.sort();
	rows
}

/// The message reported for `package`.
fn package_message<'a>(package_publish: &'a Value, package: &str) -> &'a str {
	package_publish["packages"]
		.as_array()
		.and_then(|packages| {
			packages
				.iter()
				.find(|entry| entry["package"].as_str() == Some(package))
		})
		.and_then(|entry| entry["message"].as_str())
		.unwrap_or_else(|| panic!("no message for {package}: {package_publish}"))
}

/// The deferral command reported for `package`.
fn deferral_command<'a>(package_publish: &'a Value, package: &str) -> &'a str {
	package_publish["packages"]
		.as_array()
		.and_then(|packages| {
			packages
				.iter()
				.find(|entry| entry["package"].as_str() == Some(package))
		})
		.and_then(|entry| entry["deferred_to"]["command"].as_str())
		.unwrap_or_else(|| panic!("no deferral command for {package}: {package_publish}"))
}

/// Snapshot the text and JSON dry runs of `run` under `name`.
fn snapshot_dry_run(name: &str, root: &Path, registries: &Registries, run: GitHubRun<'_>) {
	let text = publish_dry_run(root, registries, run, "text");
	assert_success(&text, "publish dry run");
	let json = publish_dry_run_json(root, registries, run);

	with_workspace_path_redacted(root, || {
		assert_snapshot!(format!("{name}_text"), stdout_of(&text));
		assert_json_snapshot!(format!("{name}_json"), json);
	});
}

/// Run `assertions` with the temporary workspace's path shown as
/// `[workspace]`, so snapshots of publish commands that name a manifest stay
/// stable.
fn with_workspace_path_redacted(root: &Path, assertions: impl FnOnce()) {
	let canonical_root = root
		.canonicalize()
		.unwrap_or_else(|error| panic!("canonicalize workspace: {error}"));
	let mut settings = insta::Settings::clone_current();

	// The canonical path (`/private/var/...` on macOS) contains the plain
	// one, so it is replaced first.
	for path in [canonical_root.as_path(), root] {
		settings.add_filter(&regex::escape(&path.to_string_lossy()), "[workspace]");
	}

	settings.bind(assertions);
}

#[rstest]
#[case::primary_tag("primary_tag", "refs/tags/v1.1.0", true)]
#[case::package_tag("package_tag", "refs/tags/app_dart/v0.2.1", false)]
#[case::unrelated_tag("unrelated_tag", "refs/tags/v9.9.9", true)]
fn publish_dry_run_scopes_the_pub_dev_package_to_its_release_tag(
	#[case] name: &str,
	#[case] git_ref: &str,
	#[case] npm_in_scope: bool,
) {
	let workspace = setup_released_workspace("workspace");
	let registries = Registries::unpublished();
	let npm_lookup = registries.npm("web");
	let pub_dev_lookup = registries.pub_dev("app_dart");

	snapshot_dry_run(
		&format!("publish_dry_run_{name}"),
		workspace.path(),
		&registries,
		GitHubRun::on(git_ref),
	);

	// A package that is out of scope for the run never reaches its registry.
	assert_eq!(npm_lookup.calls() > 0, npm_in_scope, "npm lookups");
	assert!(pub_dev_lookup.calls() > 0, "pub.dev lookups");
}

/// Finding 1 and layout L1: a pub.dev package that owns the primary tag
/// publishes on it beside the npm and cargo packages. The run on the npm
/// package's own tag publishes nothing: the primary tag run publishes every
/// package, so a workflow that starts a run for each release tag publishes
/// each package once.
#[rstest]
#[case::primary_tag(
	"primary_tag",
	"refs/tags/v0.2.1",
	&["app_dart planned", "cli planned", "web planned"],
)]
#[case::npm_tag(
	"npm_tag",
	"refs/tags/web/v1.1.0",
	&["app_dart skipped_out_of_scope", "cli skipped_out_of_scope", "web skipped_out_of_scope"],
)]
#[case::branch(
	"branch",
	"refs/heads/main",
	&["app_dart deferred_to_tag wrong_ref_type v0.2.1", "cli planned", "web planned"],
)]
fn primary_pub_dev_package_publishes_beside_every_other_package(
	#[case] name: &str,
	#[case] git_ref: &str,
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace("dart-primary");
	let registries = Registries::unpublished();
	let lookups = [
		registries.npm("web"),
		registries.pub_dev("app_dart"),
		registries.crates_io("cli"),
	];
	let run = GitHubRun::on(git_ref);
	let package_publish = publish_dry_run_json(workspace.path(), &registries, run);

	assert_eq!(package_rows(&package_publish), expected);
	snapshot_dry_run(
		&format!("dart_primary_{name}"),
		workspace.path(),
		&registries,
		run,
	);

	// A package out of scope never reaches its registry, and its line names
	// the run that publishes it.
	let in_scope = name != "npm_tag";
	assert!(
		lookups
			.iter()
			.all(|lookup| (lookup.calls() > 0) == in_scope)
	);

	if !in_scope {
		for package in ["app_dart", "cli", "web"] {
			let message = package_message(&package_publish, package);
			assert!(
				message.contains("`refs/tags/v0.2.1`"),
				"{package}: {message}"
			);
		}
	}
}

/// Finding 3 and layout L2: a run on a non-primary group's tag publishes only
/// the group's pub.dev package; the primary tag run publishes the rest.
#[rstest]
#[case::group_tag(
	"group_tag",
	"refs/tags/sdk/v1.1.0",
	&["app_dart planned", "cli skipped_out_of_scope", "web skipped_out_of_scope"],
)]
#[case::primary_tag(
	"primary_tag",
	"refs/tags/v2.0.1",
	&["app_dart deferred_to_tag wrong_ref sdk/v1.1.0", "cli planned", "web planned"],
)]
fn mixed_group_tag_publishes_only_its_pub_dev_member(
	#[case] name: &str,
	#[case] git_ref: &str,
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace("mixed-group");
	let registries = Registries::unpublished();
	let npm_lookup = registries.npm("web");
	let crates_io_lookup = registries.crates_io("cli");
	let pub_dev_lookup = registries.pub_dev("app_dart");
	let run = GitHubRun::on(git_ref);
	let package_publish = publish_dry_run_json(workspace.path(), &registries, run);

	assert_eq!(package_rows(&package_publish), expected);
	snapshot_dry_run(
		&format!("mixed_group_{name}"),
		workspace.path(),
		&registries,
		run,
	);

	let other_registries_in_scope = name == "primary_tag";
	assert_eq!(
		npm_lookup.calls() > 0,
		other_registries_in_scope,
		"npm lookups"
	);
	assert_eq!(
		crates_io_lookup.calls() > 0,
		other_registries_in_scope,
		"crates.io lookups"
	);
	assert!(pub_dev_lookup.calls() > 0, "pub.dev lookups");

	if !other_registries_in_scope {
		// The out-of-scope line names the run that publishes the package.
		for package in ["cli", "web"] {
			assert!(
				package_message(&package_publish, package).contains("`refs/tags/v2.0.1`"),
				"{package}: {}",
				package_message(&package_publish, package)
			);
		}
	}
}

/// Layout L3: no primary owner; a standalone npm package and a standalone
/// trusted pub.dev package, each with its own tag.
#[rstest]
#[case::npm_tag(
	"npm_tag",
	"refs/tags/web/v1.1.0",
	&["app_dart deferred_to_tag wrong_ref app_dart/v0.2.1", "web planned"],
)]
#[case::pub_dev_tag(
	"pub_dev_tag",
	"refs/tags/app_dart/v0.2.1",
	&["app_dart planned", "web skipped_out_of_scope"],
)]
#[case::branch(
	"branch",
	"refs/heads/main",
	&["app_dart deferred_to_tag wrong_ref_type app_dart/v0.2.1", "web planned"],
)]
fn standalone_packages_without_a_primary_tag(
	#[case] name: &str,
	#[case] git_ref: &str,
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace("no-primary");
	let registries = Registries::unpublished();
	let npm_lookup = registries.npm("web");
	let _pub_dev_lookup = registries.pub_dev("app_dart");
	let run = GitHubRun::on(git_ref);
	let package_publish = publish_dry_run_json(workspace.path(), &registries, run);

	assert_eq!(package_rows(&package_publish), expected);
	snapshot_dry_run(
		&format!("no_primary_{name}"),
		workspace.path(),
		&registries,
		run,
	);
	assert_eq!(npm_lookup.calls() > 0, name != "pub_dev_tag", "npm lookups");

	if name == "pub_dev_tag" {
		let message = package_message(&package_publish, "web");
		assert!(message.contains("`refs/tags/web/v1.1.0`"), "{message}");
	}
}

/// Layout L3 with two tag-bound packages: the group tag `sdk/v1.1.0` carries
/// a trusted pub.dev package beside an npm package, and `tools_dart` releases
/// under its own tag. Every release tag belongs to a pub.dev package, so the
/// npm package publishes on its own (group) tag and each pub.dev package on
/// its tag.
#[rstest]
#[case::group_tag(
	"group_tag",
	"refs/tags/sdk/v1.1.0",
	&["app_dart planned", "tools_dart skipped_out_of_scope", "web planned"],
)]
#[case::tools_tag(
	"tools_tag",
	"refs/tags/tools_dart/v0.2.1",
	&["app_dart skipped_out_of_scope", "tools_dart planned", "web skipped_out_of_scope"],
)]
#[case::branch(
	"branch",
	"refs/heads/main",
	&[
		"app_dart deferred_to_tag wrong_ref_type sdk/v1.1.0",
		"tools_dart deferred_to_tag wrong_ref_type tools_dart/v0.2.1",
		"web planned",
	],
)]
fn two_pub_dev_tags_publish_each_package_from_one_tag(
	#[case] name: &str,
	#[case] git_ref: &str,
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace("two-pub-dev-tags");
	let registries = Registries::unpublished();
	let _lookups = registries.every_package_unpublished();
	let run = GitHubRun::on(git_ref);
	let package_publish = publish_dry_run_json(workspace.path(), &registries, run);

	assert_eq!(package_rows(&package_publish), expected);
	snapshot_dry_run(
		&format!("two_pub_dev_tags_{name}"),
		workspace.path(),
		&registries,
		run,
	);
}

/// Each release tag starts one run, as the documented workflow does. Across
/// those runs every package of the release is planned exactly once, and every
/// package a run defers names a release tag whose run plans it.
#[rstest]
#[case::workspace("workspace")]
#[case::dart_primary("dart-primary")]
#[case::mixed_group("mixed-group")]
#[case::no_primary("no-primary")]
#[case::two_pub_dev_tags("two-pub-dev-tags")]
#[case::shared_tag_only("shared-tag-only")]
fn release_tag_runs_plan_each_package_exactly_once(#[case] fixture: &str) {
	let workspace = setup_released_workspace(fixture);
	let registries = Registries::unpublished();
	let _lookups = registries.every_package_unpublished();
	let mut planned_by = std::collections::BTreeMap::<String, Vec<String>>::new();
	let mut deferrals = Vec::new();
	let release_tags = release_record_tags(workspace.path());

	for tag in &release_tags {
		let package_publish = publish_dry_run_json(
			workspace.path(),
			&registries,
			GitHubRun::on(&format!("refs/tags/{tag}")).event("push"),
		);

		for package in package_publish["packages"]
			.as_array()
			.unwrap_or_else(|| panic!("packages array: {package_publish}"))
		{
			let name = package["package"].as_str().unwrap_or_default().to_string();
			let runs = planned_by.entry(name.clone()).or_default();

			match package["status"].as_str().unwrap_or_default() {
				"planned" => runs.push(tag.clone()),
				"deferred_to_tag" => {
					deferrals.push((
						name,
						package["deferred_to"]["tag"]
							.as_str()
							.unwrap_or_default()
							.to_string(),
					));
				}
				"skipped_out_of_scope" => {}
				status => panic!("{name} on {tag}: unexpected status {status}"),
			}
		}
	}

	for (package, runs) in &planned_by {
		assert_eq!(
			runs.len(),
			1,
			"{fixture}: {package} is planned by the runs on {runs:?} of {release_tags:?}"
		);
	}

	for (package, tag) in &deferrals {
		assert_eq!(
			planned_by.get(package).map(Vec::as_slice),
			Some([tag.clone()].as_slice()),
			"{fixture}: {package} is deferred to `{tag}`"
		);
	}
}

/// The tags the release record at `HEAD` creates.
fn release_record_tags(root: &Path) -> Vec<String> {
	let releases = root.join(".monochange/releases");
	let mut tags = std::fs::read_dir(&releases)
		.unwrap_or_else(|error| panic!("read {}: {error}", releases.display()))
		.flat_map(|entry| {
			let path = entry
				.unwrap_or_else(|error| panic!("release entry: {error}"))
				.path()
				.join("release.json");
			let record: Value = serde_json::from_str(
				&std::fs::read_to_string(&path)
					.unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
			)
			.unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
			record["release_targets"]
				.as_array()
				.unwrap_or_else(|| panic!("release targets: {record}"))
				.iter()
				.filter(|target| target["tag"].as_bool() == Some(true))
				.map(|target| target["tag_name"].as_str().unwrap_or_default().to_string())
				.collect::<Vec<_>>()
		})
		.collect::<Vec<_>>();
	tags.sort();
	tags.dedup();
	assert!(!tags.is_empty(), "the release creates no tag");
	tags
}

/// A run plans the whole release before it applies `--package`, `--group`
/// or `--ecosystem`, so selecting a package never moves it into a run that
/// another run already publishes it from.
#[rstest]
#[case::package_on_group_tag(
	"mixed-group",
	"refs/tags/sdk/v1.1.0",
	&["--package", "web"],
	&["web skipped_out_of_scope"],
)]
#[case::ecosystem_on_group_tag(
	"mixed-group",
	"refs/tags/sdk/v1.1.0",
	&["--ecosystem", "npm"],
	&["web skipped_out_of_scope"],
)]
#[case::package_on_primary_tag(
	"mixed-group",
	"refs/tags/v2.0.1",
	&["--package", "web"],
	&["web planned"],
)]
#[case::package_on_other_pub_dev_tag(
	"two-pub-dev-tags",
	"refs/tags/tools_dart/v0.2.1",
	&["--package", "web"],
	&["web skipped_out_of_scope"],
)]
#[case::ecosystem_on_npm_tag(
	"dart-primary",
	"refs/tags/web/v1.1.0",
	&["--ecosystem", "npm"],
	&["web skipped_out_of_scope"],
)]
fn selection_applies_after_the_release_is_scoped(
	#[case] fixture: &str,
	#[case] git_ref: &str,
	#[case] selection: &[&str],
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace(fixture);
	let registries = Registries::unpublished();
	let _lookups = registries.every_package_unpublished();
	let mut args = vec!["publish", "packages", "--dry-run", "--format", "json"];
	args.extend_from_slice(selection);
	let output = run_monochange(workspace.path(), &registries, GitHubRun::on(git_ref), &args);
	assert_success(&output, "publish dry run");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish json: {error}"));

	assert_eq!(package_rows(&value["package_publish"]), expected);
}

/// `--all` publishes every configured package at its current version without
/// a release record; the tags are rendered from configuration, so each
/// package is still scoped to its tag.
#[rstest]
#[case::pub_dev_tag(
	"refs/tags/app_dart/v0.2.0",
	&["app_dart planned", "web skipped_out_of_scope"],
)]
#[case::npm_tag(
	"refs/tags/web/v1.0.0",
	&["app_dart deferred_to_tag wrong_ref app_dart/v0.2.0", "web planned"],
)]
fn publish_all_without_a_release_record_scopes_to_configured_tags(
	#[case] git_ref: &str,
	#[case] expected: &[&str],
) {
	let workspace = setup_workspace("no-primary");
	let registries = Registries::unpublished();
	let _lookups = registries.every_package_unpublished();
	let output = run_monochange(
		workspace.path(),
		&registries,
		GitHubRun::on(git_ref),
		&[
			"publish",
			"packages",
			"--all",
			"--dry-run",
			"--format",
			"json",
		],
	);
	assert_success(&output, "publish --all dry run");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish json: {error}"));

	assert_eq!(package_rows(&value["package_publish"]), expected);
}

/// `--resume` keeps what an earlier run published and plans the packages it
/// left to other runs again for this run's ref: a deferral is not a
/// completed publish.
#[test]
fn resume_keeps_published_packages_and_defers_the_rest_again() {
	let workspace = setup_released_workspace("no-primary");
	let registries = Registries::unpublished();
	let _lookups = registries.every_package_unpublished();
	let run = GitHubRun::on("refs/tags/web/v1.1.0");

	// The earlier run on the npm tag published `web` and deferred `app_dart`.
	let mut previous = publish_dry_run_json(workspace.path(), &registries, run);
	previous["dry_run"] = Value::Bool(false);
	for package in previous["packages"]
		.as_array_mut()
		.unwrap_or_else(|| panic!("packages array"))
	{
		if package["package"] == "web" {
			package["status"] = Value::String("published".to_string());
		}
	}
	let resume_path = workspace.path().join("previous-publish.json");
	std::fs::write(
		&resume_path,
		serde_json::to_string_pretty(&previous)
			.unwrap_or_else(|error| panic!("serialize report: {error}")),
	)
	.unwrap_or_else(|error| panic!("write report: {error}"));

	let output = run_monochange(
		workspace.path(),
		&registries,
		run,
		&[
			"publish",
			"packages",
			"--dry-run",
			"--format",
			"json",
			"--resume",
			"previous-publish.json",
		],
	);
	assert_success(&output, "resumed publish dry run");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish json: {error}"));

	assert_eq!(
		package_rows(&value["package_publish"]),
		[
			"app_dart deferred_to_tag wrong_ref app_dart/v0.2.1",
			"web published"
		]
	);
}

/// A real run on a pub.dev package's tag mints the pub.dev OIDC token and
/// uploads only that package; the npm package, which the run on its own tag
/// publishes, never reaches npm.
#[test]
fn real_run_on_a_pub_dev_tag_uploads_only_its_package() {
	let workspace = setup_released_workspace("no-primary");
	// The release branch policy only publishes commits on `main`.
	git(workspace.path(), &["branch", "-M", "main"]);
	let registries = Registries::unpublished();
	let npm_lookup = registries.npm("web");
	let _pub_dev_lookup = registries.pub_dev("app_dart");
	let id_token = registries.server.mock(|when, then| {
		when.method(GET).path("/oidc");
		then.status(200)
			.json_body(serde_json::json!({ "value": "pub-dev-oidc-token" }));
	});
	let tool_log = workspace.path().join("tool-calls.log");
	let output = monochange_run_command(
		workspace.path(),
		&registries,
		GitHubRun::on("refs/tags/app_dart/v0.2.1").event("push"),
	)
	.env(
		"ACTIONS_ID_TOKEN_REQUEST_URL",
		format!("{}?api-version=2.0", registries.server.url("/oidc")),
	)
	.env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "request-token")
	.env("TOOL_STUB_LOG", &tool_log)
	.args(["publish", "packages", "--format", "json"])
	.output()
	.unwrap_or_else(|error| panic!("run publish: {error}"));
	assert_success(&output, "publish");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish json: {error}"));

	assert_eq!(
		package_rows(&value["package_publish"]),
		["app_dart published", "web skipped_out_of_scope"]
	);
	assert_eq!(id_token.calls(), 1, "OIDC token mints");
	assert_eq!(npm_lookup.calls(), 0, "npm lookups");
	let tool_calls = std::fs::read_to_string(&tool_log)
		.unwrap_or_else(|error| panic!("read {}: {error}", tool_log.display()));
	assert!(
		tool_calls.lines().all(|line| line.starts_with("dart ")),
		"{tool_calls}"
	);
	assert!(tool_calls.contains("dart pub publish"), "{tool_calls}");
}

/// When the release creates no tag outside a pub.dev package's tag, the run
/// on that tag is the release's only tag run, so it publishes every package.
#[test]
fn only_release_tag_publishes_every_package() {
	let workspace = setup_released_workspace("shared-tag-only");
	let registries = Registries::unpublished();
	let _lookups = [registries.npm("web"), registries.pub_dev("app_dart")];

	assert_eq!(
		package_rows(&publish_dry_run_json(
			workspace.path(),
			&registries,
			GitHubRun::on("refs/tags/sdk/v1.1.0"),
		)),
		["app_dart planned", "web planned"]
	);
}

/// Finding 4: an empty `PUB_TOKEN` (an unset secret) is no credential, so the
/// package stays bound to its tag.
#[rstest]
#[case::primary_tag(
	"refs/tags/v1.1.0",
	&["app_dart deferred_to_tag wrong_ref app_dart/v0.2.1", "web planned"],
)]
#[case::package_tag(
	"refs/tags/app_dart/v0.2.1",
	&["app_dart planned", "web skipped_out_of_scope"],
)]
fn empty_pub_token_keeps_the_package_bound_to_its_tag(
	#[case] git_ref: &str,
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace("workspace");
	let registries = Registries::unpublished();
	let _lookups = [registries.npm("web"), registries.pub_dev("app_dart")];

	assert_eq!(
		package_rows(&publish_dry_run_json(
			workspace.path(),
			&registries,
			GitHubRun::on(git_ref).pub_token(""),
		)),
		expected
	);
}

/// A non-empty `PUB_TOKEN` publishes with the token, so no run is bound to
/// the package's tag.
#[test]
fn pub_token_unbinds_the_package_from_its_tag() {
	let workspace = setup_released_workspace("workspace");
	let registries = Registries::unpublished();
	let _lookups = [registries.npm("web"), registries.pub_dev("app_dart")];

	assert_eq!(
		package_rows(&publish_dry_run_json(
			workspace.path(),
			&registries,
			GitHubRun::on("refs/tags/v1.1.0").pub_token("pub-token"),
		)),
		["app_dart planned", "web planned"]
	);
}

/// Finding 5: monochange never plans an upload pub.dev rejects because of the
/// run's event, its ref type, or a ref it cannot see.
#[rstest]
#[case::release_event_on_the_tag(
	"release_event",
	GitHubRun::on("refs/tags/app_dart/v0.2.1").event("release"),
	&["app_dart deferred_to_tag wrong_event app_dart/v0.2.1", "web skipped_out_of_scope"],
)]
#[case::create_event_on_the_tag(
	"create_event",
	GitHubRun::on("refs/tags/app_dart/v0.2.1").event("create"),
	&["app_dart deferred_to_tag wrong_event app_dart/v0.2.1", "web skipped_out_of_scope"],
)]
#[case::push_event_on_the_tag(
	"push_event",
	GitHubRun::on("refs/tags/app_dart/v0.2.1").event("push"),
	&["app_dart planned", "web skipped_out_of_scope"],
)]
#[case::pull_request(
	"pull_request",
	GitHubRun::on("refs/pull/7/merge").event("pull_request"),
	&["app_dart deferred_to_tag wrong_ref_type app_dart/v0.2.1", "web planned"],
)]
#[case::tag_ref_with_branch_ref_type(
	"branch_ref_type",
	GitHubRun::on("refs/tags/app_dart/v0.2.1").ref_type("branch"),
	&["app_dart deferred_to_tag wrong_ref_type app_dart/v0.2.1", "web planned"],
)]
#[case::missing_ref(
	"missing_ref",
	GitHubRun::on("refs/tags/v1.1.0").without_ref(),
	&["app_dart deferred_to_tag unknown_ref app_dart/v0.2.1", "web planned"],
)]
#[case::workflow_ref_without_actions_flag(
	"workflow_ref_only",
	GitHubRun::on("refs/tags/v1.1.0").without_github_actions_flag(),
	&["app_dart deferred_to_tag wrong_ref app_dart/v0.2.1", "web planned"],
)]
fn pub_dev_package_defers_when_pub_dev_would_reject_the_run(
	#[case] name: &str,
	#[case] run: GitHubRun<'static>,
	#[case] expected: &[&str],
) {
	let workspace = setup_released_workspace("workspace");
	let registries = Registries::unpublished();
	let _lookups = [registries.npm("web"), registries.pub_dev("app_dart")];
	let package_publish = publish_dry_run_json(workspace.path(), &registries, run);

	assert_eq!(package_rows(&package_publish), expected, "{name}");

	if name == "release_event" {
		// The message names what to change, not just the tag again.
		let message = package_message(&package_publish, "app_dart");
		assert!(message.contains("`release`"), "{message}");
		assert!(message.contains("`workflow_dispatch`"), "{message}");
		assert_snapshot!("wrong_event_message", message);
	}
}

/// Finding 6: the rate-limit plan for a run counts only the packages the run
/// publishes.
#[test]
fn rate_limit_plan_counts_only_packages_this_run_publishes() {
	let workspace = setup_released_workspace("workspace");
	let registries = Registries::unpublished();
	let _lookups = [registries.npm("web"), registries.pub_dev("app_dart")];
	let output = run_monochange(
		workspace.path(),
		&registries,
		GitHubRun::on("refs/tags/v1.1.0"),
		&["step", "plan-publish-rate-limits", "--format", "json"],
	);
	assert_success(&output, "plan-publish-rate-limits");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse rate-limit json: {error}"));
	let planned_registries = value["publish_rate_limits"]["windows"]
		.as_array()
		.unwrap_or_else(|| panic!("windows: {value}"))
		.iter()
		.filter(|window| window["pending"].as_u64().unwrap_or_default() > 0)
		.map(|window| window["registry"].as_str().unwrap_or_default().to_string())
		.collect::<Vec<_>>();

	assert_eq!(planned_registries, ["npm"], "{value}");
}

/// Finding 7: publish readiness checks every package of the release, whatever
/// the ref of the run that checks it.
#[rstest]
#[case::package_tag("refs/tags/app_dart/v0.2.1")]
#[case::primary_tag("refs/tags/v1.1.0")]
fn readiness_checks_every_package_on_any_ref(#[case] git_ref: &str) {
	let workspace = setup_released_workspace("readiness");
	let registries = Registries::unpublished();
	let _lookups = [
		registries.npm_published("web", "1.1.0"),
		registries.pub_dev_published("app_dart", "0.2.0"),
		registries.crates_io("cli"),
	];
	let output = run_monochange(
		workspace.path(),
		&registries,
		GitHubRun::on(git_ref),
		&["publish", "readiness", "--from", "HEAD", "--format", "json"],
	);
	assert_success(&output, "publish readiness");
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse readiness json: {error}"));
	let mut statuses = value["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages: {value}"))
		.iter()
		.map(|package| {
			format!(
				"{} {}",
				package["package"].as_str().unwrap_or_default(),
				package["status"].as_str().unwrap_or_default()
			)
		})
		.collect::<Vec<_>>();
	statuses.sort();

	assert_eq!(
		statuses,
		["app_dart ready", "cli blocked", "web already_published"],
		"{value}"
	);
}

/// Finding 8: the deferral command dispatches the workflow pub.dev trusts for
/// the package, and never renders an empty `--repo`.
#[rstest]
#[case::configured_workflow(
	REPOSITORY,
	"gh workflow run publish-dart.yml --repo example/pub-dev-release-tags --ref app_dart/v0.2.1"
)]
#[case::empty_repository("", "gh workflow run publish-dart.yml --ref app_dart/v0.2.1")]
fn deferral_command_dispatches_the_trusted_workflow(
	#[case] repository: &str,
	#[case] expected: &str,
) {
	let workspace = setup_released_workspace("dart-workflow");
	let registries = Registries::unpublished();
	let _lookups = [registries.npm("web"), registries.pub_dev("app_dart")];
	let package_publish = publish_dry_run_json(
		workspace.path(),
		&registries,
		GitHubRun::on("refs/tags/v1.1.0").repository(repository),
	);

	assert_eq!(deferral_command(&package_publish, "app_dart"), expected);
}

/// Guard: a workspace with no tag-bound package publishes exactly as before
/// on every ref. The snapshots hold the output of `origin/main` before the
/// tag-scoped publishing change.
#[rstest]
#[case::branch("branch", "refs/heads/main")]
#[case::primary_tag("primary_tag", "refs/tags/v1.1.0")]
#[case::unrelated_tag("unrelated_tag", "refs/tags/unrelated/v9.9.9")]
fn workspace_without_tag_bound_packages_publishes_as_before(
	#[case] name: &str,
	#[case] git_ref: &str,
) {
	let workspace = setup_released_workspace("no-tag-bound");
	let registries = Registries::unpublished();
	let _lookups = [
		registries.npm("web"),
		registries.pub_dev("app_dart"),
		registries.crates_io("cli"),
	];
	let run = GitHubRun::on(git_ref);
	let text = publish_dry_run(workspace.path(), &registries, run, "text");
	assert_success(&text, "publish dry run");
	let json = publish_dry_run(workspace.path(), &registries, run, "json");
	assert_success(&json, "publish dry run");

	with_workspace_path_redacted(workspace.path(), || {
		assert_snapshot!(format!("no_tag_bound_{name}_text"), stdout_of(&text));
		assert_snapshot!(format!("no_tag_bound_{name}_json"), stdout_of(&json));
	});
}

#[test]
fn validate_rejects_trusted_pub_dev_package_without_release_tag() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("missing-tag"), tempdir.path());

	let output = monochange_command(tempdir.path())
		.args(["step", "validate"])
		.output()
		.unwrap_or_else(|error| panic!("run validate: {error}"));

	assert!(
		!output.status.success(),
		"validate should fail\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);
	assert_snapshot!("validate_missing_release_tag", stderr_of(&output));
}

/// Finding 10: validation reads configuration only, so a `PUB_TOKEN` in the
/// environment does not exempt a trusted pub.dev package; the package must
/// set `publish.trusted_publishing = false`.
#[test]
fn validate_ignores_pub_token_in_the_environment() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("missing-tag"), tempdir.path());

	let output = monochange_command(tempdir.path())
		.env("PUB_TOKEN", "pub-token")
		.args(["step", "validate"])
		.output()
		.unwrap_or_else(|error| panic!("run validate: {error}"));

	assert!(
		!output.status.success(),
		"validate should fail\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);
	assert!(
		stderr_of(&output).contains("set `publish.trusted_publishing = false`"),
		"{}",
		stderr_of(&output)
	);
}

/// Finding 2: a private Dart package (`publish_to: none`) never publishes, so
/// it needs no release tag.
#[test]
fn validate_accepts_private_dart_package_without_release_tag() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("private-package"), tempdir.path());

	let output = monochange_command(tempdir.path())
		.args(["step", "validate"])
		.output()
		.unwrap_or_else(|error| panic!("run validate: {error}"));

	assert_success(&output, "validate");
}
