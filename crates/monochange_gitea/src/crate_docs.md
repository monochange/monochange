# `monochange_gitea`

<!-- {=monochangeGiteaCrateDocs} -->

`monochange_gitea` turns `monochange` release manifests into Gitea automation requests.

Reach for this crate when you want to preview or publish Gitea releases and release pull requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive Gitea release payloads and release-PR bodies from `monochange`'s structured release manifest
- keep Gitea automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building Gitea release automation on top of prepared monochange release state
- previewing would-be Gitea releases and release PRs in CI before publishing
- self-hosted Gitea instances that need the same release workflow as GitHub or GitLab

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a pull-request payload for the release
- `validate_source_configuration(source)` validates Gitea-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeGiteaCrateDocs} -->
