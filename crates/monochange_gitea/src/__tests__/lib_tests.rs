use std::collections::BTreeMap;
use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::MockServer;
use monochange_core::BumpSeverity;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestPlanDecision;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseOwnerKind;
use monochange_core::SourceCapabilities;
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::VersionFormat;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output;
use tempfile::tempdir;

fn gitea_client() -> MonochangeResult<Client> {
	build_http_client("Gitea")
}

use super::*;

#[test]
fn gitea_source_capabilities_capture_provider_limits() {
	assert_eq!(
		source_capabilities(),
		SourceCapabilities {
			draft_releases: true,
			prereleases: true,
			generated_release_notes: false,
			auto_merge_change_requests: false,
			released_issue_comments: false,
			requires_host: true,
		}
	);
}

#[test]
fn validate_source_configuration_rejects_missing_host_and_unsupported_features() {
	let error = validate_source_configuration(&sample_source(None, None))
		.err()
		.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("[source].host must be set for `provider = \"gitea\"`")
	);

	let error = validate_source_configuration(&SourceConfiguration {
		pull_requests: ProviderMergeRequestSettings {
			auto_merge: true,
			..ProviderMergeRequestSettings::default()
		},
		..sample_source(None, Some("https://codeberg.org".to_string()))
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("[source.pull_requests].auto_merge is not supported")
	);

	let error = validate_source_configuration(&SourceConfiguration {
		releases: ProviderReleaseSettings {
			source: ProviderReleaseNotesSource::GitHubGenerated,
			..ProviderReleaseSettings::default()
		},
		..sample_source(None, Some("https://codeberg.org".to_string()))
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("provider-generated release notes are not supported")
	);
}

#[test]
fn publish_release_requests_creates_release_via_gitea_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(404);
	});
	let create = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v1/repos/org/monochange/releases");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://codeberg.org/org/monochange/releases/tag/v1.2.0\"}");
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_gitea_env(Some("token"), || {
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(publish_release_requests(&source, &requests))
			.unwrap_or_else(|error| panic!("publish release: {error}"))
	});

	lookup.assert();
	create.assert();
	assert_eq!(
		outcomes
			.first()
			.unwrap_or_else(|| panic!("expected outcome"))
			.operation,
		SourceReleaseOperation::Created,
	);
}

#[test]
fn publish_release_requests_updates_existing_release_via_gitea_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://codeberg.org/org/monochange/releases/tag/v1.2.0\"}");
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://codeberg.org/org/monochange/releases/tag/v1.2.0\"}");
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_gitea_env(Some("token"), || {
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(publish_release_requests(&source, &requests))
			.unwrap_or_else(|error| panic!("publish release: {error}"))
	});

	lookup.assert();
	update.assert();
	assert_eq!(
		outcomes
			.first()
			.unwrap_or_else(|| panic!("expected outcome"))
			.operation,
		SourceReleaseOperation::Updated,
	);
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
fn publish_release_pull_request_creates_pull_request_and_labels() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/pulls")
			.query_param("state", "open")
			.query_param("head", "org:monochange/release/release")
			.query_param("base", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create = server.mock(|when, then| {
		when.method(POST).path("/api/v1/repos/org/monochange/pulls");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\"}");
	});
	let labels = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v1/repos/org/monochange/issues/12/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let (_tempdir, repo) = seed_git_repository();
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let mut request = build_release_pull_request_request(&source, &sample_manifest());
	request.commit_message.body = Some("release body".to_string());

	let outcome = with_gitea_env(Some("token"), || {
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(publish_release_pull_request(
				&source,
				&repo,
				&request,
				&[PathBuf::from("release.txt")],
				false,
				false,
			))
			.unwrap_or_else(|error| panic!("publish pull request: {error}"))
	});

	list.assert();
	create.assert();
	labels.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Created);
	assert!(
		!git_output(
			&repo,
			&["rev-parse", "--verify", "monochange/release/release"]
		)
		.trim()
		.is_empty()
	);
	let commit_body = git_output(&repo, &["log", "-1", "--pretty=%B"]);
	assert!(commit_body.contains("release body"));
}

#[test]
fn publish_pull_request_updates_existing_pull_request() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/pulls")
			.query_param("state", "open")
			.query_param("head", "org:monochange/release/release")
			.query_param("base", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"[{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\",\"title\":\"old title\",\"body\":\"old body\",\"base\":{\"ref\":\"main\"},\"head\":{\"sha\":\"old-sha\"},\"labels\":[]}]",
			);
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v1/repos/org/monochange/pulls/12");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\"}");
	});
	let labels = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v1/repos/org/monochange/issues/12/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let request = build_release_pull_request_request(
		&sample_source(
			Some(format!("{}/api/v1", server.base_url())),
			Some("https://codeberg.org".to_string()),
		),
		&sample_manifest(),
	);
	let client = gitea_client().unwrap_or_else(|error| panic!("client: {error}"));
	let headers = auth_headers("token").unwrap_or_else(|error| panic!("headers: {error}"));

	let outcome = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(publish_pull_request(
			&client,
			&headers,
			&format!("{}/api/v1", server.base_url()),
			&request,
		))
		.unwrap_or_else(|error| panic!("update pull request: {error}"));

	list.assert();
	update.assert();
	labels.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Updated);
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
fn publish_release_pull_request_skips_push_when_existing_pull_request_matches_local_head() {
	let server = MockServer::start();
	let (_tempdir, repo) = seed_git_repository();
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let request = build_release_pull_request_request(&source, &sample_manifest());

	git(&repo, &["checkout", "-B", &request.head_branch]);
	git(&repo, &["add", "-A", "--", "release.txt"]);
	git(&repo, &["commit", "-m", "prepare release branch"]);
	git(&repo, &["push", "-u", "origin", &request.head_branch]);
	let head_commit = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_string();
	let labels = request
		.labels
		.iter()
		.map(|label| format!("{{\"name\":{label:?}}}"))
		.collect::<Vec<_>>()
		.join(",");
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/pulls")
			.query_param("state", "open")
			.query_param("head", "org:monochange/release/release")
			.query_param("base", "main");
			then.status(200)
				.header("content-type", "application/json")
				.body(format!(
					"[{{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\",\"title\":{title:?},\"body\":{body:?},\"base\":{{\"ref\":{base:?}}},\"head\":{{\"sha\":{head:?}}},\"labels\":[{labels}]}}]",
					title = request.title,
					body = request.body,
					base = request.base_branch,
					labels = labels,
					head = head_commit,
				));
		});
	git(
		&repo,
		&[
			"remote",
			"set-url",
			"origin",
			"/definitely/missing/gitea-origin.git",
		],
	);

	let outcome = with_gitea_env(Some("token"), || {
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(publish_release_pull_request(
				&source,
				&repo,
				&request,
				&[PathBuf::from("release.txt")],
				false,
				false,
			))
			.unwrap_or_else(|error| panic!("publish pull request: {error}"))
	});

	list.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Skipped);
	assert_eq!(outcome.number, 12);
}

fn sample_source(api_url: Option<String>, host: Option<String>) -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::Gitea,
		owner: "org".to_string(),
		repo: "monochange".to_string(),
		host,
		api_url,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn sample_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: true,
		version: Some("1.2.0".to_string()),
		group_version: Some("1.2.0".to_string()),
		release_targets: vec![ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.2.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.0".to_string(),
			rendered_title: "test title".to_string(),
			rendered_changelog_title: "test changelog title".to_string(),
			floating_tags: Vec::new(),
			members: vec!["core".to_string(), "app".to_string()],
		}],
		package_publications: vec![],
		released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changesets: Vec::new(),
		changelogs: vec![ReleaseManifestChangelog {
			owner_id: "sdk".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			output: "default".to_string(),
			stream: "default".to_string(),
			path: PathBuf::from("changelog.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "1.2.0".to_string(),
				summary: vec!["Grouped release for `sdk`.".to_string()],
				sections: vec![ReleaseNotesSection {
					title: "Features".to_string(),
					collapsed: false,
					entries: vec!["add gitea publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add gitea publishing"
					.to_string(),
		}],
		deleted_changesets: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![ReleaseManifestPlanDecision {
				package: "core".to_string(),
				bump: BumpSeverity::Minor,
				trigger: "changeset".to_string(),
				planned_version: Some("1.2.0".to_string()),
				reasons: vec!["add provider automation".to_string()],
				upstream_sources: vec!["sdk".to_string()],
			}],
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	}
}

fn with_gitea_env<R>(token: Option<&str>, action: impl FnOnce() -> R) -> R {
	temp_env::with_vars([("GITEA_TOKEN", token)], action)
}

fn seed_git_repository() -> (tempfile::TempDir, PathBuf) {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let bare = tempdir.path().join("origin.git");
	let repo = tempdir.path().join("repo");
	git(
		tempdir.path(),
		&[
			"init",
			"--bare",
			"--initial-branch=main",
			bare.to_string_lossy().as_ref(),
		],
	);
	git(
		tempdir.path(),
		&[
			"init",
			"--initial-branch=main",
			repo.to_string_lossy().as_ref(),
		],
	);
	git(&repo, &["config", "user.name", "monochange Tests"]);
	git(&repo, &["config", "user.email", "monochange@example.com"]);
	git(&repo, &["config", "commit.gpgsign", "false"]);

	std::fs::write(repo.join("release.txt"), "before\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(&repo, &["add", "release.txt"]);
	git(&repo, &["commit", "-m", "initial"]);
	git(
		&repo,
		&["remote", "add", "origin", bare.to_string_lossy().as_ref()],
	);
	git(&repo, &["push", "-u", "origin", "main"]);
	std::fs::write(repo.join("release.txt"), "after\n")
		.unwrap_or_else(|error| panic!("update release file: {error}"));
	(tempdir, repo)
}
