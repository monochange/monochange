# `monochange_hosting`

<!-- {=monochangeHostingCrateDocs} -->

`monochange_hosting` packages the shared git and HTTP plumbing used by hosted source providers.

Reach for this crate when you are implementing GitHub, Gitea, Forgejo, or GitLab release adapters and want one place for release-body rendering, change-request branch naming, JSON requests, and git branch orchestration.

## Why use it?

- keep provider adapters focused on provider-specific payloads instead of repeated plumbing
- share one markdown rendering path for release bodies and release pull requests
- reuse one set of blocking HTTP helpers with consistent error messages

## Best for

- implementing or testing hosted source adapters
- generating release pull request bodies from prepared manifests
- staging, committing, and pushing release branches through shared wrappers

## Public entry points

- `release_body(source, manifest, target)` resolves the outward release body for a target
- `release_pull_request_body(manifest)` renders the provider change-request body
- `release_pull_request_branch(prefix, command)` normalizes the change-request branch name
- `get_json`, `post_json`, `patch_json`, and `put_json` wrap provider API requests
- `git_checkout_branch`, `git_stage_paths`, `git_commit_paths`, and `git_push_branch` wrap shared git operations

<!-- {/monochangeHostingCrateDocs} -->
