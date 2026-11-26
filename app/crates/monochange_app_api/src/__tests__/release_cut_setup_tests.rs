#![allow(clippy::disallowed_methods)]

use std::sync::OnceLock;

use httpmock::Method;
use httpmock::MockServer;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;

use super::*;
use crate::AppSecrets;
use crate::github_app::GitHubAppAuth;

async fn test_state(server: &MockServer) -> AppState {
	static TEST_KEY: OnceLock<String> = OnceLock::new();

	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "test-jwt-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	let pem = TEST_KEY.get_or_init(|| {
		RsaPrivateKey::new(&mut OsRng, 2048)
			.unwrap()
			.to_pkcs1_pem(LineEnding::LF)
			.unwrap()
			.to_string()
	});
	state.github_app = Some(GitHubAppAuth::new(
		"123",
		pem,
		"test-webhook-secret",
		&server.base_url(),
	));
	state.github_api_origin = server.base_url();
	state
}

fn mock_installation_token(server: &MockServer, installation_id: i64) {
	server.mock(|when, then| {
		when.method(Method::POST).path(format!(
			"/app/installations/{installation_id}/access_tokens"
		));
		then.status(201).json_body(serde_json::json!({
			"token": "ghs_test_token",
			"expires_at": "2099-01-01T00:00:00Z",
		}));
	});
}

#[test]
fn enable_release_cuts_in_monochange_toml_preserves_existing_sections_and_detects_adoption() {
	let original = r#"# Existing repository comment
[source]
provider = "github"
owner = "acme"
repo = "monorepo"

[source.releases]
branches = ["main"]

[package.core]
path = "crates/core"
type = "cargo"
"#;
	assert!(!already_uses_release_cuts(original));

	let updated =
		enable_release_cuts_in_monochange_toml(Some(original), "acme", "monorepo", "main").unwrap();
	assert!(already_uses_release_cuts(&updated));
	assert!(updated.contains("# Existing repository comment"));
	assert!(updated.contains("[package.core]"));
	assert!(updated.contains("strategy = \"cut\""));
	assert!(updated.contains("release/*"));
	assert!(updated.contains("trigger = \"sticky\""));
	assert!(updated.contains("mode = \"semantic_pr\""));

	let from_scratch =
		enable_release_cuts_in_monochange_toml(None, "acme", "fresh", "trunk").unwrap();
	assert!(already_uses_release_cuts(&from_scratch));
	assert!(from_scratch.contains("base = \"trunk\""));
}

#[test]
fn agent_prompt_and_workflow_include_repository_and_default_branch() {
	let prompt = build_release_cut_agent_prompt("acme/monorepo", "9 PRs merged in 1 hour", "trunk");
	assert!(prompt.contains("acme/monorepo"));
	assert!(prompt.contains("9 PRs merged in 1 hour"));
	assert!(prompt.contains("base = \"trunk\""));
	assert!(prompt.contains("strategy = \"cut\""));
	assert!(prompt.contains(GUIDE_URL));

	let workflow = render_release_cut_workflow_yaml("trunk");
	assert!(workflow.contains("- trunk"));
	assert!(workflow.contains("refs/heads/trunk"));
	assert!(workflow.contains("sync-release: true"));
}

#[tokio::test]
async fn detect_suggestion_backfills_from_github_and_skips_repos_already_on_cut_strategy() {
	let server = MockServer::start();
	let state = test_state(&server).await;
	mock_installation_token(&server, 77);

	let now = 1_760_000_000_i64;
	let pulls: Vec<serde_json::Value> = (0..8)
		.map(|index| {
			let ts = chrono::DateTime::from_timestamp(now - (7 - index) * 400, 0).unwrap();
			serde_json::json!({
				"number": index + 10,
				"html_url": format!("https://github.com/acme/fast/pull/{}", index + 10),
				"merged_at": ts.to_rfc3339(),
				"head": { "ref": format!("feat/change-{index}") }
			})
		})
		.collect();

	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/fast");
		then.status(200)
			.json_body(serde_json::json!({ "default_branch": "main" }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/fast/pulls")
			.query_param("state", "closed")
			.query_param("base", "main");
		then.status(200).json_body(serde_json::Value::Array(pulls));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/fast/contents/monochange.toml");
		then.status(200).body(
			"[source]\nprovider = \"github\"\nowner = \"acme\"\nrepo = \"fast\"\n\n[source.pull_requests]\nstrategy = \"rolling\"\n",
		);
	});

	let suggestion = detect_repository_release_cut_suggestion(&state, 501, 77, "acme/fast", now)
		.await
		.unwrap_or_else(|| panic!("expected high-velocity suggestion"));
	assert_eq!(suggestion.repository, "acme/fast");
	assert_eq!(suggestion.peak_prs_in_hour, 8);
	assert_eq!(suggestion.merged_prs_24h, 8);
	assert_eq!(suggestion.guide_url, GUIDE_URL);
	assert!(suggestion.agent_prompt.contains("acme/fast"));

	// A repo already configured with strategy = "cut" is skipped even if DB has a burst.
	for index in 0..8 {
		record_merged_pull_request(&state.db, 502, index + 1, "main", now - index * 300)
			.await
			.unwrap();
	}
	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/adopted");
		then.status(200)
			.json_body(serde_json::json!({ "default_branch": "main" }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/adopted/contents/monochange.toml");
		then.status(200)
			.body("[source.pull_requests]\nstrategy = \"cut\"\n");
	});

	let skipped =
		detect_repository_release_cut_suggestion(&state, 502, 77, "acme/adopted", now).await;
	assert!(skipped.is_none());
}

#[tokio::test]
async fn open_release_cut_setup_pull_request_commits_config_and_workflow_and_opens_pr() {
	let server = MockServer::start();
	let state = test_state(&server).await;
	mock_installation_token(&server, 88);

	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/fast");
		then.status(200)
			.json_body(serde_json::json!({ "default_branch": "main" }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/fast/git/ref/heads/main");
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": "base123" } }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/fast/contents/monochange.toml");
		then.status(200)
			.body("[package.core]\npath = \".\"\ntype = \"cargo\"\n");
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/fast/git/commits/base123");
		then.status(200)
			.json_body(serde_json::json!({ "tree": { "sha": "basetree123" } }));
	});
	let create_tree = server.mock(|when, then| {
		when.method(Method::POST).path("/repos/acme/fast/git/trees");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "newtree456" }));
	});
	let create_commit = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/fast/git/commits");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "newcommit789" }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path(format!("/repos/acme/fast/git/ref/heads/{SETUP_BRANCH}"));
		then.status(404);
	});
	let create_ref = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/fast/git/refs")
			.json_body(serde_json::json!({
				"ref": format!("refs/heads/{SETUP_BRANCH}"),
				"sha": "newcommit789",
			}));
		then.status(201).json_body(serde_json::json!({}));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/fast/pulls")
			.query_param("state", "open")
			.query_param("head", format!("acme:{SETUP_BRANCH}"));
		then.status(200).json_body(serde_json::json!([]));
	});
	let create_pr = server.mock(|when, then| {
		when.method(Method::POST).path("/repos/acme/fast/pulls");
		then.status(201).json_body(serde_json::json!({
			"number": 42,
			"html_url": "https://github.com/acme/fast/pull/42",
		}));
	});

	let result = open_release_cut_setup_pull_request(&state, 88, "acme/fast")
		.await
		.unwrap();
	assert_eq!(result.pull_request_number, 42);
	assert_eq!(
		result.pull_request_url,
		"https://github.com/acme/fast/pull/42"
	);
	assert_eq!(result.branch, SETUP_BRANCH);
	assert!(result.created);
	assert!(result.workflow_committed);

	create_tree.assert();
	create_commit.assert();
	create_ref.assert();
	create_pr.assert();
}

#[tokio::test]
async fn open_release_cut_setup_pull_request_falls_back_when_workflows_permission_missing() {
	let server = MockServer::start();
	let state = test_state(&server).await;
	mock_installation_token(&server, 89);

	server.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/noworkflow");
		then.status(200)
			.json_body(serde_json::json!({ "default_branch": "main" }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/noworkflow/git/ref/heads/main");
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": "base123" } }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/noworkflow/contents/monochange.toml");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/noworkflow/git/commits/base123");
		then.status(200)
			.json_body(serde_json::json!({ "tree": { "sha": "basetree123" } }));
	});
	// Tree containing workflow fails with 403; retry with only monochange.toml succeeds.
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/noworkflow/git/trees")
			.body_includes(SETUP_WORKFLOW_PATH);
		then.status(403)
			.json_body(serde_json::json!({ "message": "refusing to allow workflow file" }));
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/noworkflow/git/trees");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "configonlytree" }));
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/noworkflow/git/commits");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "configonlycommit" }));
	});
	server.mock(|when, then| {
		when.method(Method::GET).path(format!(
			"/repos/acme/noworkflow/git/ref/heads/{SETUP_BRANCH}"
		));
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/noworkflow/git/refs");
		then.status(201).json_body(serde_json::json!({}));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/noworkflow/pulls");
		then.status(200).json_body(serde_json::json!([]));
	});
	let create_pr = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/noworkflow/pulls")
			.body_includes("workflows: write");
		then.status(201).json_body(serde_json::json!({
			"number": 7,
			"html_url": "https://github.com/acme/noworkflow/pull/7",
		}));
	});

	let result = open_release_cut_setup_pull_request(&state, 89, "acme/noworkflow")
		.await
		.unwrap();
	assert_eq!(result.pull_request_number, 7);
	assert!(!result.workflow_committed);
	create_pr.assert();
}
