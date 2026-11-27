use std::collections::BTreeMap;
use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::Method::PUT;
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

fn gitlab_client() -> MonochangeResult<Client> {
	build_http_client("GitLab")
}

use super::*;

#[test]
fn gitlab_source_capabilities_capture_provider_limits() {
	assert_eq!(
		source_capabilities(),
		SourceCapabilities {
			draft_releases: false,
			prereleases: false,
			generated_release_notes: false,
			auto_merge_change_requests: false,
			released_issue_comments: false,
			requires_host: false,
		}
	);
}

#[test]
fn validate_source_configuration_rejects_unsupported_gitlab_features() {
	let error = validate_source_configuration(&SourceConfiguration {
		pull_requests: ProviderMergeRequestSettings {
			auto_merge: true,
			..ProviderMergeRequestSettings::default()
		},
		..sample_source(None)
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
			draft: true,
			..ProviderReleaseSettings::default()
		},
		..sample_source(None)
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("[source.releases].draft is not supported")
	);

	let error = validate_source_configuration(&SourceConfiguration {
		releases: ProviderReleaseSettings {
			prerelease: true,
			..ProviderReleaseSettings::default()
		},
		..sample_source(None)
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("[source.releases].prerelease is not supported")
	);

	let error = validate_source_configuration(&SourceConfiguration {
		releases: ProviderReleaseSettings {
			source: ProviderReleaseNotesSource::GitHubGenerated,
			..ProviderReleaseSettings::default()
		},
		..sample_source(None)
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
fn gitlab_json_helpers_cover_not_found_and_status_errors() {
	let server = MockServer::start();
	let missing = server.mock(|when, then| {
		when.method(GET).path("/missing");
		then.status(404);
	});
	let failing_get = server.mock(|when, then| {
		when.method(GET).path("/fail-get");
		then.status(500);
	});
	let failing_post = server.mock(|when, then| {
		when.method(POST).path("/fail-post");
		then.status(500);
	});
	let failing_put = server.mock(|when, then| {
		when.method(PUT).path("/fail-put");
		then.status(500);
	});
	let failing_patch = server.mock(|when, then| {
		when.method(PATCH).path("/fail-patch");
		then.status(500);
	});
	let client = gitlab_client().unwrap_or_else(|error| panic!("client: {error}"));
	let base = server.base_url();

	let headers = auth_headers("token").unwrap_or_else(|error| panic!("headers: {error}"));

	let missing_value = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(get_optional_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/missing"),
			"GitLab",
		))
		.unwrap_or_else(|error| panic!("optional json: {error}"));
	assert_eq!(missing_value, None);

	let get_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(get_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-get"),
			"GitLab",
		))
		.err()
		.unwrap_or_else(|| panic!("expected get error"));
	assert!(get_error.to_string().contains("GitLab API GET"));

	let post_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(post_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-post"),
			&serde_json::json!({"tag_name": "v1.2.3"}),
			"GitLab",
		))
		.err()
		.unwrap_or_else(|| panic!("expected post error"));
	assert!(post_error.to_string().contains("GitLab API POST"));

	let put_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(put_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-put"),
			&serde_json::json!({"title": "Release"}),
			"GitLab",
		))
		.err()
		.unwrap_or_else(|| panic!("expected put error"));
	assert!(put_error.to_string().contains("GitLab API PUT"));

	let patch_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(patch_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-patch"),
			&serde_json::json!({"name": "v1.2.3"}),
			"GitLab",
		))
		.err()
		.unwrap_or_else(|| panic!("expected patch error"));
	assert!(patch_error.to_string().contains("GitLab API PATCH"));

	missing.assert();
	failing_get.assert();
	failing_post.assert();
	failing_put.assert();
	failing_patch.assert();
}

#[test]
fn publish_release_requests_creates_release_via_gitlab_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(404);
	});
	let create = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/group%2Fmonochange/releases");
		then.status(201)
			.header("content-type", "application/json")
			.body(
				"{\"web_url\":\"https://gitlab.example.com/group/monochange/-/releases/v1.2.0\"}",
			);
	});
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let manifest = sample_manifest();
	let requests = build_release_requests(&source, &manifest);
	let outcomes = with_gitlab_env(Some("token"), || {
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
fn publish_release_requests_updates_existing_release_via_gitlab_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"web_url\":\"https://gitlab.example.com/releases/v1.2.0\"}");
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"web_url\":\"https://gitlab.example.com/releases/v1.2.0\"}");
	});
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_gitlab_env(Some("token"), || {
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
fn publish_release_pull_request_creates_merge_request_and_pushes_branch() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests")
			.query_param("state", "opened")
			.query_param("source_branch", "monochange/release/release")
			.query_param("target_branch", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests");
		then.status(201)
			.header("content-type", "application/json")
			.body(
				"{\"iid\":12,\"web_url\":\"https://gitlab.example.com/group/monochange/-/merge_requests/12\"}",
			);
	});
	let (_tempdir, repo) = seed_git_repository();
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let mut request = build_release_pull_request_request(&source, &sample_manifest());
	request.commit_message.body = Some("release body".to_string());

	let outcome = with_gitlab_env(Some("token"), || {
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
			.unwrap_or_else(|error| panic!("publish merge request: {error}"))
	});

	list.assert();
	create.assert();
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
fn publish_merge_request_updates_existing_merge_request() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests")
			.query_param("state", "opened")
			.query_param("source_branch", "monochange/release/release")
			.query_param("target_branch", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"[{\"iid\":12,\"web_url\":\"https://gitlab.example.com/group/monochange/-/merge_requests/12\",\"title\":\"old title\",\"description\":\"old body\",\"target_branch\":\"main\",\"labels\":[],\"sha\":\"old-sha\"}]",
			);
	});
	let update = server.mock(|when, then| {
		when.method(PUT)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests/12");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"iid\":12,\"web_url\":\"https://gitlab.example.com/group/monochange/-/merge_requests/12\"}",
			);
	});
	let request = build_release_pull_request_request(
		&sample_source(Some(format!("{}/api/v4", server.base_url()))),
		&sample_manifest(),
	);
	let client = gitlab_client().unwrap_or_else(|error| panic!("client: {error}"));
	let headers = auth_headers("token").unwrap_or_else(|error| panic!("headers: {error}"));

	let outcome = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(publish_merge_request(
			&client,
			&headers,
			&format!("{}/api/v4", server.base_url()),
			&request,
		))
		.unwrap_or_else(|error| panic!("update merge request: {error}"));

	list.assert();
	update.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Updated);
}

fn sample_source(api_url: Option<String>) -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitLab,
		owner: "group".to_string(),
		repo: "monochange".to_string(),
		host: Some("https://gitlab.com".to_string()),
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
					entries: vec!["add gitlab publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add gitlab publishing"
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

fn with_gitlab_env<R>(token: Option<&str>, action: impl FnOnce() -> R) -> R {
	temp_env::with_vars([("GITLAB_TOKEN", token), ("GL_TOKEN", None)], action)
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
