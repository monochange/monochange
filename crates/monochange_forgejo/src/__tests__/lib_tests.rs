#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::MockServer;
use monochange_core::BumpSeverity;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetRevision;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedSourceAdapter;
use monochange_core::HostedSourceFeatures;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::PreparedChangeset;
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
use monochange_test_helpers::install_rustls_ring_provider;
use tempfile::tempdir;

use super::*;

#[test]
fn forgejo_source_capabilities_capture_provider_limits() {
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

#[tokio::test(flavor = "multi_thread")]
async fn hosted_source_adapter_reports_forgejo_features_and_delegates_context_annotation() {
	let adapter: &dyn HostedSourceAdapter = &HOSTED_SOURCE_ADAPTER;
	let source = sample_source(None, Some("https://codeberg.org".to_string()));
	let mut changesets = Vec::new();

	assert_eq!(adapter.provider(), SourceProvider::Forgejo);
	assert_eq!(adapter.features(), HostedSourceFeatures::default());
	adapter.annotate_changeset_context(&source, &mut changesets);
	adapter
		.enrich_changeset_context(&source, &mut changesets)
		.await;
}

#[test]
fn validate_source_configuration_rejects_missing_host_and_unsupported_features() {
	let error = validate_source_configuration(&sample_source(None, None))
		.err()
		.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("[source].host must be set for `provider = \"forgejo\"`")
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
fn forgejo_context_annotation_updates_hosts_commits_and_authors() {
	let source = sample_source(
		None,
		Some("https://codeberg.org/org/monochange/subpath".to_string()),
	);
	assert_eq!(forgejo_host_name(&source).as_deref(), Some("codeberg.org"));
	assert_eq!(
		forgejo_commit_url(&source, "abc1234567890"),
		"https://codeberg.org/org/monochange/subpath/org/monochange/commit/abc1234567890"
	);
	let mut changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("feature".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GenericGit,
			host: None,
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					id: None,
					login: Some("ifiokjr".to_string()),
					display_name: Some("Ifiok Jr.".to_string()),
					url: None,
					source: HostedActorSourceKind::CommitAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					sha: "abc1234567890".to_string(),
					short_sha: "abc1234".to_string(),
					url: None,
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: None,
			}),
			last_updated: None,
			related_issues: Vec::new(),
		}),
	}];

	annotate_changeset_context(&source, &mut changesets);
	enrich_changeset_context(&source, &mut changesets);

	let context = changesets
		.first()
		.and_then(|changeset| changeset.context.as_ref())
		.unwrap_or_else(|| panic!("expected changeset context"));
	assert_eq!(context.provider, HostingProviderKind::Forgejo);
	assert_eq!(context.host.as_deref(), Some("codeberg.org"));
	assert_eq!(
		context
			.introduced
			.as_ref()
			.and_then(|revision| revision.commit.as_ref())
			.and_then(|commit| commit.url.as_deref()),
		Some("https://codeberg.org/org/monochange/subpath/org/monochange/commit/abc1234567890")
	);
	assert_eq!(
		context
			.introduced
			.as_ref()
			.and_then(|revision| revision.actor.as_ref())
			.map(|actor| (actor.provider, actor.host.clone())),
		Some((
			HostingProviderKind::Forgejo,
			Some("codeberg.org".to_string())
		))
	);
}

#[test]
fn forgejo_json_helpers_cover_not_found_and_status_errors() {
	let server = MockServer::start();
	let optional_success = server.mock(|when, then| {
		when.method(GET).path("/optional-success");
		then.status(200).body(r#"{"ok":true}"#);
	});
	let missing = server.mock(|when, then| {
		when.method(GET).path("/missing");
		then.status(404);
	});
	let get_success = server.mock(|when, then| {
		when.method(GET).path("/get-success");
		then.status(200).body(r#"{"ok":true}"#);
	});
	let failing_get = server.mock(|when, then| {
		when.method(GET).path("/fail-get");
		then.status(500);
	});
	let failing_post = server.mock(|when, then| {
		when.method(POST).path("/fail-post");
		then.status(500);
	});
	let failing_patch = server.mock(|when, then| {
		when.method(PATCH).path("/fail-patch");
		then.status(500);
	});
	let invalid_optional = server.mock(|when, then| {
		when.method(GET).path("/invalid-optional");
		then.status(200).body("not json");
	});

	let invalid_get = server.mock(|when, then| {
		when.method(GET).path("/invalid-get");
		then.status(200).body("not json");
	});
	let invalid_post = server.mock(|when, then| {
		when.method(POST).path("/invalid-post");
		then.status(200).body("not json");
	});
	let invalid_patch = server.mock(|when, then| {
		when.method(PATCH).path("/invalid-patch");
		then.status(200).body("not json");
	});
	install_rustls_ring_provider();
	let client = Client::builder()
		.build()
		.expect("failed to build Forgejo HTTP client");
	let base = server.base_url();
	let headers = auth_headers("token").unwrap_or_else(|error| panic!("headers: {error}"));

	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("failed to build Forgejo runtime");
	runtime.block_on(async {
		let optional_value = get_optional_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/optional-success"),
			"Forgejo",
		)
		.await
		.unwrap_or_else(|error| panic!("optional success json: {error}"));
		assert_eq!(
			optional_value.and_then(|value| value.get("ok").cloned()),
			Some(serde_json::json!(true))
		);

		let missing_value = get_optional_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/missing"),
			"Forgejo",
		)
		.await
		.unwrap_or_else(|error| panic!("optional json: {error}"));
		assert_eq!(missing_value, None);

		let send_error =
			get_optional_json::<serde_json::Value>(&client, &headers, "http://", "Forgejo")
				.await
				.err()
				.unwrap_or_else(|| panic!("expected optional get send error"));
		assert!(send_error.to_string().contains("Forgejo API GET"));

		let invalid_optional_error = get_optional_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/invalid-optional"),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected optional invalid json error"));
		assert!(
			invalid_optional_error
				.to_string()
				.contains("Forgejo API GET")
		);

		let get_value = get_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/get-success"),
			"Forgejo",
		)
		.await
		.unwrap_or_else(|error| panic!("get success json: {error}"));
		assert_eq!(get_value.get("ok").cloned(), Some(serde_json::json!(true)));

		let get_send_error = get_json::<serde_json::Value>(&client, &headers, "http://", "Forgejo")
			.await
			.err()
			.unwrap_or_else(|| panic!("expected get send error"));
		assert!(get_send_error.to_string().contains("Forgejo API GET"));

		let get_error = get_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-get"),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected get error"));
		assert!(get_error.to_string().contains("Forgejo API GET"));

		let invalid_get_error = get_json::<serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/invalid-get"),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected invalid get json error"));
		assert!(invalid_get_error.to_string().contains("Forgejo API GET"));

		let post_send_error = post_json::<_, serde_json::Value>(
			&client,
			&headers,
			"http://",
			&serde_json::json!({}),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected post send error"));
		assert!(post_send_error.to_string().contains("Forgejo API POST"));

		let post_error = post_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-post"),
			&serde_json::json!({"tag_name": "v1.2.3"}),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected post error"));
		assert!(post_error.to_string().contains("Forgejo API POST"));

		let invalid_post_error = post_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/invalid-post"),
			&serde_json::json!({"tag_name": "v1.2.3"}),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected invalid post json error"));
		assert!(invalid_post_error.to_string().contains("Forgejo API POST"));

		let patch_send_error = patch_json::<_, serde_json::Value>(
			&client,
			&headers,
			"http://",
			&serde_json::json!({}),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected patch send error"));
		assert!(patch_send_error.to_string().contains("Forgejo API PATCH"));

		let patch_error = patch_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/fail-patch"),
			&serde_json::json!({"name": "v1.2.3"}),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected patch error"));
		assert!(patch_error.to_string().contains("Forgejo API PATCH"));

		let invalid_patch_error = patch_json::<_, serde_json::Value>(
			&client,
			&headers,
			&format!("{base}/invalid-patch"),
			&serde_json::json!({"name": "v1.2.3"}),
			"Forgejo",
		)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected invalid patch json error"));
		assert!(
			invalid_patch_error
				.to_string()
				.contains("Forgejo API PATCH")
		);
	});

	optional_success.assert();
	missing.assert();
	get_success.assert();
	failing_get.assert();
	failing_post.assert();
	failing_patch.assert();
	invalid_optional.assert();
	invalid_get.assert();

	invalid_post.assert();
	invalid_patch.assert();
}

#[test]
fn publish_release_requests_creates_release_via_forgejo_api() {
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
			.body(r#"{"html_url":"https://codeberg.org/org/monochange/releases/tag/v1.2.0"}"#);
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_forgejo_env(Some("token"), || {
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
fn publish_release_requests_updates_existing_release_via_forgejo_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"html_url":"https://codeberg.org/org/monochange/releases/tag/v1.2.0"}"#);
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"html_url":"https://codeberg.org/org/monochange/releases/tag/v1.2.0"}"#);
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_forgejo_env(Some("token"), || {
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
			.body(r#"{"number":12,"html_url":"https://codeberg.org/org/monochange/pulls/12"}"#);
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

	let outcome = with_forgejo_env(Some("token"), || {
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
			.body(r#"[{"number":12,"html_url":"https://codeberg.org/org/monochange/pulls/12","title":"old title","body":"old body","base":{"ref":"main"},"head":{"sha":"old-sha"},"labels":[]}]"#);
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v1/repos/org/monochange/pulls/12");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"number":12,"html_url":"https://codeberg.org/org/monochange/pulls/12"}"#);
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
	install_rustls_ring_provider();
	let client = Client::builder()
		.build()
		.expect("failed to build Forgejo HTTP client");
	let headers = auth_headers("token").unwrap_or_else(|error| panic!("headers: {error}"));

	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("failed to build Forgejo runtime");
	let outcome = runtime
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
		.map(|label| format!(r#"{{"name":{label:?}}}"#))
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
					r#"[{{"number":12,"html_url":"https://codeberg.org/org/monochange/pulls/12","title":{title:?},"body":{body:?},"base":{{"ref":{base:?}}},"head":{{"sha":{head:?}}},"labels":[{labels}]}}]"#,
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
			"/definitely/missing/forgejo-origin.git",
		],
	);

	let outcome = with_forgejo_env(Some("token"), || {
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
		provider: SourceProvider::Forgejo,
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
					entries: vec!["add forgejo publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add forgejo publishing"
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

fn with_forgejo_env<R>(token: Option<&str>, action: impl FnOnce() -> R) -> R {
	temp_env::with_vars([("FORGEJO_TOKEN", token)], action)
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
