# `monochange_gitlab`

<!-- {=monochangeGitlabCrateDocs} -->

`monochange_gitlab` turns `monochange` release manifests into GitLab automation requests.

Reach for this crate when you want to preview or publish GitLab releases and merge requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive GitLab release payloads and merge-request bodies from `monochange`'s structured release manifest
- keep GitLab automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building GitLab release automation on top of prepared monochange release state
- previewing would-be GitLab releases and merge requests in CI before publishing
- self-hosted GitLab instances that need the same release workflow as GitHub

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a merge-request payload for the release
- `validate_source_configuration(source)` validates GitLab-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeGitlabCrateDocs} -->
