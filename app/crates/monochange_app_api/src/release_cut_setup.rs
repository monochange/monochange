//! High-velocity release cut detection, agent prompt generation, and one-click
//! setup pull requests for `monochange.dev`.

use chrono::DateTime;
use monochange_app_db::velocity::VELOCITY_LOOKBACK_SECS;
use monochange_app_db::velocity::detect_high_velocity_burst;
use monochange_app_db::velocity::recent_merged_timestamps;
use monochange_app_db::velocity::record_merged_pull_request;
use monochange_shared::ReleaseCutSetupResult;
use monochange_shared::ReleaseCutSuggestion;
use serde::Deserialize;
use thiserror::Error;
use toml_edit::Array;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::Table;
use toml_edit::value;

use crate::AppState;
use crate::github_app::GitHubAppError;

/// Relative path to the frozen release cuts guide in the embedded book.
pub const GUIDE_URL: &str = "/book/guide/16-frozen-release-cuts";

/// Branch name used by the one-click setup pull request action.
pub const SETUP_BRANCH: &str = "monochange/enable-release-cuts";

/// Title for the one-click setup pull request.
pub const SETUP_PR_TITLE: &str =
	"chore(release): adopt frozen release cuts for high-velocity merges";

/// Workflow file path created by the one-click setup pull request.
pub const SETUP_WORKFLOW_PATH: &str = ".github/workflows/release-cut.yml";

/// Errors returned by the one-click release-cut setup flow.
#[derive(Debug, Error)]
pub enum ReleaseCutSetupError {
	#[error("GitHub App is not configured on this deployment")]
	AppUnavailable,
	#[error("invalid repository name `{0}`")]
	InvalidRepository(String),
	#[error("`{0}` already enables `strategy = \"cut\"` in `monochange.toml`")]
	AlreadyEnabled(String),
	#[error("could not update `monochange.toml`: {0}")]
	InvalidConfig(String),
	#[error("GitHub API error: {0}")]
	GitHub(#[from] GitHubAppError),
}

#[derive(Debug, Deserialize)]
struct RepoMeta {
	default_branch: String,
}

#[derive(Debug, Deserialize)]
struct RefObj {
	object: GitObjectRef,
}

#[derive(Debug, Deserialize)]
struct GitObjectRef {
	sha: String,
}

#[derive(Debug, Deserialize)]
struct CommitObj {
	tree: GitObjectRef,
}

#[derive(Debug, Deserialize)]
struct CreatedSha {
	sha: String,
}

#[derive(Debug, Deserialize)]
struct PullSummary {
	number: u64,
	html_url: String,
	#[serde(default)]
	merged_at: Option<String>,
	#[serde(default)]
	head: Option<PullHead>,
}

#[derive(Debug, Deserialize)]
struct PullHead {
	#[serde(rename = "ref", default)]
	ref_name: String,
}

/// Return `true` when `monochange_toml` already configures
/// `[source.pull_requests].strategy = "cut"`.
#[must_use]
pub fn already_uses_release_cuts(monochange_toml: &str) -> bool {
	let Ok(document) = monochange_toml.parse::<DocumentMut>() else {
		return false;
	};
	document
		.get("source")
		.and_then(Item::as_table)
		.and_then(|source| source.get("pull_requests"))
		.and_then(Item::as_table)
		.and_then(|pull_requests| pull_requests.get("strategy"))
		.and_then(Item::as_str)
		.is_some_and(|strategy| strategy.eq_ignore_ascii_case("cut"))
}

/// Patch an existing `monochange.toml` (or generate a starter configuration)
/// to enable sticky frozen release cuts (`strategy = "cut"`) while preserving
/// existing sections, packages, groups, and comments.
pub fn enable_release_cuts_in_monochange_toml(
	existing_toml: Option<&str>,
	owner: &str,
	repo: &str,
	default_branch: &str,
) -> Result<String, String> {
	let mut document = match existing_toml {
		Some(content) => {
			content
				.parse::<DocumentMut>()
				.map_err(|error| format!("parse error: {error}"))?
		}
		None => DocumentMut::new(),
	};

	let source = ensure_table(document.as_table_mut(), "source");
	if !source.contains_key("provider") {
		source["provider"] = value("github");
	}
	if !source.contains_key("owner") {
		source["owner"] = value(owner);
	}
	if !source.contains_key("repo") {
		source["repo"] = value(repo);
	}

	let releases = ensure_table(source, "releases");
	if !releases.contains_key("enabled") {
		releases["enabled"] = value(true);
	}
	ensure_branch_pattern(releases, "release/*");
	if !releases.contains_key("enforce_for_tags") {
		releases["enforce_for_tags"] = value(true);
	}
	if !releases.contains_key("enforce_for_publish") {
		releases["enforce_for_publish"] = value(true);
	}

	let pull_requests = ensure_table(source, "pull_requests");
	if !pull_requests.contains_key("enabled") {
		pull_requests["enabled"] = value(true);
	}
	pull_requests["strategy"] = value("cut");
	if !pull_requests.contains_key("base") {
		pull_requests["base"] = value(default_branch);
	}
	if !pull_requests.contains_key("branch_prefix") {
		pull_requests["branch_prefix"] = value("monochange/release");
	}
	if !pull_requests.contains_key("title") {
		pull_requests["title"] = value("chore(release): prepare release {{ cut_id }}");
	}

	let cut = ensure_table(pull_requests, "cut");
	if !cut.contains_key("branch_template") {
		cut["branch_template"] = value("release/{{ cut_id }}");
	}
	if !cut.contains_key("trigger") {
		cut["trigger"] = value("sticky");
	}
	if !cut.contains_key("source_commit") {
		cut["source_commit"] = value("latest_green");
	}
	if !cut.contains_key("refresh_command") {
		cut["refresh_command"] = value("/refresh");
	}
	if !cut.contains_key("status_banner") {
		cut["status_banner"] = value(true);
	}
	if !cut.contains_key("retention") {
		cut["retention"] = value("delete");
	}

	let sync = ensure_table(pull_requests, "sync");
	if !sync.contains_key("mode") {
		sync["mode"] = value("semantic_pr");
	}
	if !sync.contains_key("branch") {
		sync["branch"] = value("monochange/sync-release");
	}
	if !sync.contains_key("title") {
		sync["title"] = value("chore(release): sync {{ cut_id }} to {{ base }}");
	}
	if !sync.contains_key("auto_merge") {
		sync["auto_merge"] = value(true);
	}

	let mut rendered = document.to_string();
	if !rendered.ends_with('\n') {
		rendered.push('\n');
	}
	Ok(rendered)
}

fn ensure_table<'a>(parent: &'a mut Table, key: &str) -> &'a mut Table {
	if !parent.get(key).is_some_and(Item::is_table) {
		let mut table = Table::new();
		table.set_implicit(false);
		parent.insert(key, Item::Table(table));
	}
	parent
		.get_mut(key)
		.and_then(Item::as_table_mut)
		.unwrap_or_else(|| unreachable!("table was just inserted"))
}

fn ensure_branch_pattern(releases: &mut Table, pattern: &str) {
	if let Some(existing) = releases.get_mut("branches").and_then(Item::as_array_mut) {
		let has_pattern = existing.iter().any(|entry| entry.as_str() == Some(pattern));
		if !has_pattern {
			existing.push(pattern);
		}
		return;
	}
	let mut array = Array::new();
	array.push(pattern);
	releases["branches"] = value(array);
}

/// Render a ready-to-commit GitHub Actions workflow implementing sticky frozen
/// release cuts, `/refresh` comment triggers, immediate publish from
/// `release/*`, and non-blocking post-release sync back to `default_branch`.
#[must_use]
pub fn render_release_cut_workflow_yaml(default_branch: &str) -> String {
	format!(
		r#"name: Release Cuts

on:
  push:
    branches:
      - {default_branch}
      - release/*
  issue_comment:
    types: [created]
  workflow_dispatch:
    inputs:
      refresh:
        description: Re-cut the open release PR from the newest green commit
        type: boolean
        default: false
      from:
        description: Optional base commit SHA or ref to cut from
        type: string
        default: ""

permissions:
  contents: write
  pull-requests: write
  issues: write
  id-token: write

jobs:
  cut-or-banner:
    if: >-
      (github.event_name == 'push' && github.ref == 'refs/heads/{default_branch}') ||
      github.event_name == 'workflow_dispatch' ||
      (
        github.event_name == 'issue_comment' &&
        github.event.issue.pull_request != null &&
        contains(fromJSON('["OWNER","MEMBER","COLLABORATOR"]'), github.event.comment.author_association) &&
        contains(github.event.comment.body, '/refresh')
      )
    runs-on: ubuntu-latest
    concurrency:
      group: monochange-release-cut
      cancel-in-progress: false
    steps:
      # Local pushes with GITHUB_TOKEN do not trigger pull_request CI checks
      # (including on `/refresh`). Use the hosted monochange GitHub App backend
      # via `id-token: write`, or supply a PAT / App token in MONOCHANGE_GITHUB_TOKEN.
      - uses: actions/checkout@v4
        with:
          ref: {default_branch}
          fetch-depth: 0
          token: ${{{{ secrets.MONOCHANGE_GITHUB_TOKEN || secrets.GITHUB_TOKEN }}}}
      - name: Cut frozen release PR or update status banner
        uses: monochange/actions@v0
        env:
          GITHUB_TOKEN: ${{{{ secrets.MONOCHANGE_GITHUB_TOKEN || secrets.GITHUB_TOKEN }}}}
        with:
          name: release-pr
          token: ${{{{ secrets.MONOCHANGE_GITHUB_TOKEN || secrets.GITHUB_TOKEN }}}}
          refresh: ${{{{ github.event_name == 'issue_comment' || inputs.refresh || false }}}}
          from: ${{{{ inputs.from || '' }}}}
          minimum-reviewer-permission: write

  tag-publish-and-sync:
    # Ignore initial frozen cut branch creation (`created == true`) and branch
    # deletion (`deleted == true`), then confirm HEAD is an actual release commit
    # before publishing (so `/refresh` branch moves never trigger publish).
    if: >-
      github.event_name == 'push' &&
      startsWith(github.ref, 'refs/heads/release/') &&
      !github.event.created &&
      !github.event.deleted
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          token: ${{{{ secrets.MONOCHANGE_GITHUB_TOKEN || secrets.GITHUB_TOKEN }}}}
      - name: Detect release commit
        id: release_commit
        shell: bash
        run: |
          set -euo pipefail
          is_release=false
          if git rev-parse --verify HEAD^1 >/dev/null 2>&1; then
            for record in $(git diff-tree --no-commit-id --name-only -r --diff-filter=AM HEAD^1 HEAD -- '.monochange/releases/*/release.json'); do
              if [ -f "$record" ] && ! grep -q '"synced_from_commit"' "$record"; then
                is_release=true
                break
              fi
            done
          fi
          echo "is_release_commit=$is_release" >> "$GITHUB_OUTPUT"
      - name: Tag, publish, and sync back to {default_branch}
        if: steps.release_commit.outputs.is_release_commit == 'true'
        uses: monochange/actions@v0
        env:
          GITHUB_TOKEN: ${{{{ secrets.MONOCHANGE_GITHUB_TOKEN || secrets.GITHUB_TOKEN }}}}
        with:
          name: post-merge-release
          ref: HEAD
          token: ${{{{ secrets.MONOCHANGE_GITHUB_TOKEN || secrets.GITHUB_TOKEN }}}}
          sync-release: true
          open-next-cut: true
"#
	)
}

/// Build a self-contained prompt that developers can copy and paste into their
/// coding agent to migrate `full_name` onto frozen release cuts.
#[must_use]
pub fn build_release_cut_agent_prompt(
	full_name: &str,
	burst_summary: &str,
	default_branch: &str,
) -> String {
	let (owner, repo) = full_name.split_once('/').unwrap_or((full_name, full_name));
	format!(
		r#"Switch `{full_name}` from rolling release PRs to monochange's frozen release cut workflow (`strategy = "cut"`).

Why: monochange.dev observed high merge velocity on `{default_branch}` ({burst_summary}). With rolling release PRs, frequent merges onto `{default_branch}` overwrite the release PR head, restart CI, and block fast-forward merges before CI completes.

Please make the following changes in this repository:

1. Update `monochange.toml` so releases cut from a frozen green commit onto `release/{{ cut_id }}`, stay frozen while open (only updating a status banner on subsequent pushes to `{default_branch}`), support `/refresh` comments, and sync cleanly back to `{default_branch}` after publish:

```toml
[source]
provider = "github"
owner = "{owner}"
repo = "{repo}"

[source.releases]
enabled = true
branches = ["release/*"]
enforce_for_tags = true
enforce_for_publish = true

[source.pull_requests]
enabled = true
strategy = "cut"
base = "{default_branch}"
branch_prefix = "monochange/release"
title = "chore(release): prepare release {{{{ cut_id }}}}"

[source.pull_requests.cut]
branch_template = "release/{{{{ cut_id }}}}"
trigger = "sticky"
source_commit = "latest_green"
refresh_command = "/refresh"
status_banner = true
retention = "delete"

[source.pull_requests.sync]
mode = "semantic_pr" # Or "merge_pr" to put tags in `git log {default_branch}`, or "bookkeeping_only" with `[defaults].version_source = "tag"`
branch = "monochange/sync-release"
title = "chore(release): sync {{{{ cut_id }}}} to {{{{ base }}}}"
auto_merge = true
```

2. Update or add `.github/workflows/release-cut.yml` (replacing any rolling release-PR workflow) so:
   - Pushes to `{default_branch}`, `workflow_dispatch`, and `/refresh` `issue_comment` events run `monochange/actions` with `name: release-pr`.
   - Pushes to `release/*` run `monochange/actions` with `name: post-merge-release`, `sync-release: true`, and `open-next-cut: true`.
3. Keep existing package/group definitions, publish steps, and lockfile commands intact.
4. Run `monochange step validate` and `monochange run release --dry-run` to confirm the cut branch plan renders cleanly.

Reference guide: https://monochange.dev{GUIDE_URL}
"#
	)
}

/// Evaluate whether `full_name` crossed the high-velocity merge threshold in
/// the last 24 hours and is not yet using `strategy = "cut"`.
pub async fn detect_repository_release_cut_suggestion(
	state: &AppState,
	github_repo_id: i64,
	github_installation_id: i64,
	full_name: &str,
	now_unix: i64,
) -> Option<ReleaseCutSuggestion> {
	let since_unix = now_unix.saturating_sub(VELOCITY_LOOKBACK_SECS);
	let mut timestamps = recent_merged_timestamps(&state.db, github_repo_id, since_unix)
		.await
		.unwrap_or_default();
	let mut burst = detect_high_velocity_burst(&timestamps, now_unix);
	let mut default_branch = "main".to_owned();
	let mut cached_token: Option<String> = None;

	// If local webhook rows don't show a burst yet and GitHub App credentials are
	// available, inspect recent merged PRs from GitHub and persist them locally.
	if burst.is_none()
		&& let Some(app) = state.github_app.as_ref()
		&& let Ok(token) = app
			.installation_token(&state.http, github_installation_id)
			.await
	{
		if let Ok(branch) =
			fetch_default_branch(&state.http, &state.github_api_origin, &token, full_name).await
		{
			default_branch = branch;
		}
		if let Ok(github_merges) = recent_merged_prs_from_github(
			&state.http,
			&state.github_api_origin,
			&token,
			full_name,
			&default_branch,
			since_unix,
		)
		.await
		{
			for (pr_number, merged_at_unix) in github_merges {
				let _ = record_merged_pull_request(
					&state.db,
					github_repo_id,
					pr_number,
					&default_branch,
					merged_at_unix,
				)
				.await;
				timestamps.push(merged_at_unix);
			}
			burst = detect_high_velocity_burst(&timestamps, now_unix);
		}
		cached_token = Some(token);
	}

	let burst = burst?;

	// Skip repositories that already adopted `strategy = "cut"` in `monochange.toml`.
	if let Some(app) = state.github_app.as_ref() {
		let token = match cached_token {
			Some(token) => Some(token),
			None => {
				app.installation_token(&state.http, github_installation_id)
					.await
					.ok()
			}
		};
		if let Some(token) = token {
			if default_branch == "main"
				&& let Ok(branch) =
					fetch_default_branch(&state.http, &state.github_api_origin, &token, full_name)
						.await
			{
				default_branch = branch;
			}
			if let Ok(Some(config)) = read_repository_file(
				&state.http,
				&state.github_api_origin,
				&token,
				full_name,
				"monochange.toml",
				&default_branch,
			)
			.await && already_uses_release_cuts(&config)
			{
				return None;
			}
		}
	}

	let burst_summary = burst.format_summary();
	let agent_prompt = build_release_cut_agent_prompt(full_name, &burst_summary, &default_branch);

	Some(ReleaseCutSuggestion {
		repository: full_name.to_owned(),
		peak_prs_in_hour: burst.peak_prs_in_hour,
		merged_prs_24h: burst.merged_prs_24h,
		burst_summary,
		guide_url: GUIDE_URL.to_owned(),
		agent_prompt,
	})
}

/// Open (or update) a pull request enabling `strategy = "cut"` on `full_name`.
pub async fn open_release_cut_setup_pull_request(
	state: &AppState,
	github_installation_id: i64,
	full_name: &str,
) -> Result<ReleaseCutSetupResult, ReleaseCutSetupError> {
	let Some(app) = state.github_app.as_ref() else {
		return Err(ReleaseCutSetupError::AppUnavailable);
	};
	let Some((owner, repo)) = full_name.split_once('/') else {
		return Err(ReleaseCutSetupError::InvalidRepository(
			full_name.to_owned(),
		));
	};
	if owner.is_empty() || repo.is_empty() || repo.contains('/') {
		return Err(ReleaseCutSetupError::InvalidRepository(
			full_name.to_owned(),
		));
	}

	let token = app
		.installation_token(&state.http, github_installation_id)
		.await?;
	let api_url = state.github_api_origin.trim_end_matches('/');
	let default_branch = fetch_default_branch(&state.http, api_url, &token, full_name).await?;
	let base_sha = fetch_branch_sha(&state.http, api_url, &token, full_name, &default_branch)
		.await?
		.ok_or_else(|| {
			GitHubAppError::Status(
				"read default branch",
				reqwest::StatusCode::NOT_FOUND,
				format!("branch `{default_branch}` not found"),
			)
		})?;
	let existing_config = read_repository_file(
		&state.http,
		api_url,
		&token,
		full_name,
		"monochange.toml",
		&default_branch,
	)
	.await?;
	if existing_config
		.as_deref()
		.is_some_and(already_uses_release_cuts)
	{
		return Err(ReleaseCutSetupError::AlreadyEnabled(full_name.to_owned()));
	}

	let updated_config = enable_release_cuts_in_monochange_toml(
		existing_config.as_deref(),
		owner,
		repo,
		&default_branch,
	)
	.map_err(ReleaseCutSetupError::InvalidConfig)?;
	let workflow_yaml = render_release_cut_workflow_yaml(&default_branch);

	let files_with_workflow = vec![
		("monochange.toml", updated_config.as_str()),
		(SETUP_WORKFLOW_PATH, workflow_yaml.as_str()),
	];
	let workflow_committed = match commit_setup_branch(
		&state.http,
		api_url,
		&token,
		full_name,
		&base_sha,
		&files_with_workflow,
	)
	.await
	{
		Ok(()) => true,
		Err(GitHubAppError::Status(
			_,
			reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::UNPROCESSABLE_ENTITY,
			_,
		)) => {
			commit_setup_branch(
				&state.http,
				api_url,
				&token,
				full_name,
				&base_sha,
				&[("monochange.toml", updated_config.as_str())],
			)
			.await?;
			false
		}
		Err(error) => return Err(ReleaseCutSetupError::GitHub(error)),
	};

	let pr_body = render_setup_pr_body(&default_branch, workflow_committed, &workflow_yaml);
	let (number, url, created) = upsert_setup_pull_request(
		&state.http,
		api_url,
		&token,
		full_name,
		owner,
		&default_branch,
		&pr_body,
	)
	.await?;

	Ok(ReleaseCutSetupResult {
		repository: full_name.to_owned(),
		pull_request_number: number,
		pull_request_url: url,
		branch: SETUP_BRANCH.to_owned(),
		created,
		workflow_committed,
	})
}

fn render_setup_pr_body(
	default_branch: &str,
	workflow_committed: bool,
	workflow_yaml: &str,
) -> String {
	let workflow_note = if workflow_committed {
		format!(
			"- Added `{SETUP_WORKFLOW_PATH}` to cut frozen release PRs, refresh them via `/refresh`, publish from `release/*`, and sync back to `{default_branch}`."
		)
	} else {
		format!(
			"- The monochange GitHub App does not yet have `workflows: write` permission on this repository, so `{SETUP_WORKFLOW_PATH}` was not committed automatically. Add the workflow below to this branch before merging:\n\n```yaml\n{workflow_yaml}```"
		)
	};

	format!(
		r#"## Why this pull request was opened

High merge velocity was detected on `{default_branch}` (more than 1 merged PR every 10 minutes sustained over at least an hour). Under the default rolling release PR workflow, merges arriving faster than CI finishes repeatedly overwrite the release PR head and restart checks.

This PR switches the repository to monochange's **sticky frozen release cut** strategy (`strategy = "cut"`):

1. When pending changesets exist on `{default_branch}`, monochange cuts a frozen branch `release/<cut_id>` from the latest green commit and opens a Cut PR (`monochange/release/<cut_id>` -> `release/<cut_id>`).
2. While the Cut PR is open, new pushes to `{default_branch}` never overwrite the branch or restart CI; monochange only updates a live status banner on the PR.
3. Comment `/refresh` on the Cut PR whenever you want to pull newer commits into the cut.
4. Merging the Cut PR fast-forwards `release/<cut_id>`, tags and publishes immediately without pausing `{default_branch}`, and opens a clean non-blocking sync PR back to `{default_branch}`.

## What changed

- Updated `monochange.toml` with `[source.releases]`, `[source.pull_requests].strategy = "cut"`, `[source.pull_requests.cut]`, and `[source.pull_requests.sync]`.
{workflow_note}

## Verification

- Run `monochange step validate` to verify the updated configuration.
- Read the full guide at https://monochange.dev{GUIDE_URL}.
"#
	)
}

async fn fetch_default_branch(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	full_name: &str,
) -> Result<String, GitHubAppError> {
	let response = http
		.get(format!(
			"{}/repos/{full_name}",
			api_url.trim_end_matches('/')
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("read repository", status, body));
	}
	let meta: RepoMeta = response.json().await?;
	Ok(meta.default_branch)
}

async fn fetch_branch_sha(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	full_name: &str,
	branch: &str,
) -> Result<Option<String>, GitHubAppError> {
	let response = http
		.get(format!(
			"{}/repos/{full_name}/git/ref/heads/{branch}",
			api_url.trim_end_matches('/')
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;
	if response.status() == reqwest::StatusCode::NOT_FOUND {
		return Ok(None);
	}
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("read git ref", status, body));
	}
	let reference: RefObj = response.json().await?;
	Ok(Some(reference.object.sha))
}

async fn read_repository_file(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	full_name: &str,
	path: &str,
	git_ref: &str,
) -> Result<Option<String>, GitHubAppError> {
	let response = http
		.get(format!(
			"{}/repos/{full_name}/contents/{path}",
			api_url.trim_end_matches('/')
		))
		.query(&[("ref", git_ref)])
		.bearer_auth(token)
		.header("Accept", "application/vnd.github.raw+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;
	if response.status() == reqwest::StatusCode::NOT_FOUND {
		return Ok(None);
	}
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("read repository file", status, body));
	}
	Ok(Some(response.text().await?))
}

async fn recent_merged_prs_from_github(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	full_name: &str,
	default_branch: &str,
	since_unix: i64,
) -> Result<Vec<(i64, i64)>, GitHubAppError> {
	let response = http
		.get(format!(
			"{}/repos/{full_name}/pulls",
			api_url.trim_end_matches('/')
		))
		.query(&[
			("state", "closed"),
			("base", default_branch),
			("sort", "updated"),
			("direction", "desc"),
			("per_page", "100"),
		])
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status(
			"list recent pull requests",
			status,
			body,
		));
	}
	let pulls: Vec<PullSummary> = response.json().await?;
	let mut merged = Vec::new();
	for pull in pulls {
		let head_ref = pull.head.as_ref().map_or("", |head| head.ref_name.as_str());
		if head_ref.starts_with("monochange/") {
			continue;
		}
		let Some(merged_at) = pull.merged_at.as_deref() else {
			continue;
		};
		let Ok(parsed) = DateTime::parse_from_rfc3339(merged_at) else {
			continue;
		};
		let ts = parsed.timestamp();
		if ts >= since_unix {
			merged.push((i64::try_from(pull.number).unwrap_or(i64::MAX), ts));
		}
	}
	Ok(merged)
}

async fn commit_setup_branch(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	full_name: &str,
	base_sha: &str,
	files: &[(&str, &str)],
) -> Result<(), GitHubAppError> {
	let base_commit_resp = http
		.get(format!(
			"{api_url}/repos/{full_name}/git/commits/{base_sha}"
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;
	if !base_commit_resp.status().is_success() {
		let status = base_commit_resp.status();
		let body = base_commit_resp.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("read base commit", status, body));
	}
	let base_commit: CommitObj = base_commit_resp.json().await?;

	let tree_entries: Vec<serde_json::Value> = files
		.iter()
		.map(|(path, content)| {
			serde_json::json!({
				"path": path,
				"mode": "100644",
				"type": "blob",
				"content": content,
			})
		})
		.collect();
	let tree_resp = http
		.post(format!("{api_url}/repos/{full_name}/git/trees"))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&serde_json::json!({
			"base_tree": base_commit.tree.sha,
			"tree": tree_entries,
		}))
		.send()
		.await?;
	if !tree_resp.status().is_success() {
		let status = tree_resp.status();
		let body = tree_resp.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("create setup tree", status, body));
	}
	let tree: CreatedSha = tree_resp.json().await?;

	let commit_resp = http
		.post(format!("{api_url}/repos/{full_name}/git/commits"))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&serde_json::json!({
			"message": SETUP_PR_TITLE,
			"tree": tree.sha,
			"parents": [base_sha],
		}))
		.send()
		.await?;
	if !commit_resp.status().is_success() {
		let status = commit_resp.status();
		let body = commit_resp.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("create setup commit", status, body));
	}
	let commit: CreatedSha = commit_resp.json().await?;

	let existing_branch = fetch_branch_sha(http, api_url, token, full_name, SETUP_BRANCH).await?;
	let ref_resp = if existing_branch.is_some() {
		http.patch(format!(
			"{api_url}/repos/{full_name}/git/refs/heads/{SETUP_BRANCH}"
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&serde_json::json!({
			"sha": commit.sha,
			"force": true,
		}))
		.send()
		.await?
	} else {
		http.post(format!("{api_url}/repos/{full_name}/git/refs"))
			.bearer_auth(token)
			.header("Accept", "application/vnd.github+json")
			.header("User-Agent", "monochange")
			.json(&serde_json::json!({
				"ref": format!("refs/heads/{SETUP_BRANCH}"),
				"sha": commit.sha,
			}))
			.send()
			.await?
	};
	if !ref_resp.status().is_success() {
		let status = ref_resp.status();
		let body = ref_resp.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status("update setup branch", status, body));
	}
	Ok(())
}

async fn upsert_setup_pull_request(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	full_name: &str,
	owner: &str,
	default_branch: &str,
	body: &str,
) -> Result<(u64, String, bool), GitHubAppError> {
	let head_selector = format!("{owner}:{SETUP_BRANCH}");
	let list_resp = http
		.get(format!("{api_url}/repos/{full_name}/pulls"))
		.query(&[
			("state", "open"),
			("head", head_selector.as_str()),
			("base", default_branch),
		])
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;
	if list_resp.status().is_success() {
		let existing: Vec<PullSummary> = list_resp.json().await?;
		if let Some(pull) = existing.into_iter().next() {
			let update_resp = http
				.patch(format!("{api_url}/repos/{full_name}/pulls/{}", pull.number))
				.bearer_auth(token)
				.header("Accept", "application/vnd.github+json")
				.header("User-Agent", "monochange")
				.json(&serde_json::json!({
					"title": SETUP_PR_TITLE,
					"body": body,
				}))
				.send()
				.await?;
			if !update_resp.status().is_success() {
				let status = update_resp.status();
				let body = update_resp.text().await.unwrap_or_default();
				return Err(GitHubAppError::Status(
					"update setup pull request",
					status,
					body,
				));
			}
			let updated: PullSummary = update_resp.json().await?;
			return Ok((updated.number, updated.html_url, false));
		}
	}

	let create_resp = http
		.post(format!("{api_url}/repos/{full_name}/pulls"))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&serde_json::json!({
			"title": SETUP_PR_TITLE,
			"head": SETUP_BRANCH,
			"base": default_branch,
			"body": body,
		}))
		.send()
		.await?;
	if !create_resp.status().is_success() {
		let status = create_resp.status();
		let body = create_resp.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status(
			"create setup pull request",
			status,
			body,
		));
	}
	let created: PullSummary = create_resp.json().await?;
	Ok((created.number, created.html_url, true))
}

#[cfg(test)]
#[path = "__tests__/release_cut_setup_tests.rs"]
mod tests;
