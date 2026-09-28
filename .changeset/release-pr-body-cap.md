---
"monochange": major
"monochange_core": major
"monochange_config": minor
"monochange_hosting": minor
"monochange_github": minor
"monochange_gitlab": patch
"monochange_gitea": patch
"monochange_forgejo": patch
---

# Bound the release request body so it cannot outgrow the provider limit

> **Breaking change:** `SourceChangeRequest` gained a required `body_truncation` field, so exhaustive struct literals no longer compile without it.
>
> Set it to `None` when you build a request by hand. The field is optional on the wire, so JSON written by an older version still deserializes, and provider request builders populate it for you.

```rust
let request = SourceChangeRequest {
	// ...
	body_truncation: None,
};
```

A release pull request that stayed open while changesets kept merging grew its body through the update call, which providers do not bound the way they bound the create call. GitHub then rejected the create call that replaces the pull request with `status 422` and `body is too long (maximum: 65536 characters)`, at step 5/5 and with no mention of a setting that fixes it.

Every provider now bounds the rendered body on both paths.

Three public changes in `monochange_core`:

```rust
// before
pub struct ProviderMergeRequestSettings {
	pub enabled: bool,
	pub branch_prefix: String,
	pub base: String,
	pub title: String,
	pub commit_subject: Option<String>,
	pub labels: Vec<String>,
	pub auto_merge: bool,
	pub verified_commits: bool,
}

// after
pub struct ProviderMergeRequestSettings {
	// ...
	pub body_style: ProviderPullRequestBodyStyle,
	pub max_body_chars: Option<usize>,
}

// New: how much of the release notes the request body carries.
pub enum ProviderPullRequestBodyStyle {
	Full,
	Summary,
}

// New helper, plus a per-provider default on `SourceProvider`.
let limit = settings.effective_max_body_chars(SourceProvider::GitHub); // Some(65_536)
```

`SourceChangeRequest` gained a `body_truncation: Option<SourceChangeRequestBodyTruncation>` field, so consumers of the JSON release request can see when and how much the body was shortened instead of inferring it from the length.

```rust
pub struct SourceChangeRequestBodyTruncation {
	pub max_chars: usize,
	pub original_chars: usize,
	pub dropped_entries: usize,
}
```

`monochange_hosting` gained `render_release_pull_request_body`, which returns the bounded body plus the truncation report, and `release_pull_request_body_for_source`, which applies the configured style and limit. `monochange_github` previously carried private copies of `release_pull_request_body` and `release_pull_request_branch` that were byte-identical to the shared versions; both now come from `monochange_hosting`, so the GitHub and non-GitHub renderers cannot drift apart again.

When GitHub rejects a create call for body length, the error now names the setting instead of only echoing the API payload:

```text
GitHub API POST `/repos/{owner}/{repo}/pulls` failed: status 422; ...;
the rendered release pull request body exceeded GitHub's 65536 character limit. Set
`[source.pull_requests].body_style = "summary"` ... or lower `[source.pull_requests].max_body_chars`
```

`[source.pull_requests].max_body_chars` must be greater than `0`; `monochange_config` rejects a zero value at load time.
