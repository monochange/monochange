# `PublishReadiness`

## What it does

`PublishReadiness` checks package-registry publishing readiness without publishing packages.

It reads package publications from a release commit, compares them with the current workspace configuration and target registries, and reports which packages are ready, already published, or unsupported by built-in publishing.

## Why use it

Use `PublishReadiness` as a reviewable preflight before mutating registry state with package publishing.

It is especially useful for:

- CI jobs that should prove a release can publish before credentials are available
- human review of a package-publish plan
- generating a JSON readiness artifact for `PlanPublishRateLimits` in publish mode
- resuming after partial registry publication, because already-published versions are reported as resumable instead of blocking

## Inputs

- `from` — required tag or commit-ish used to locate the release record
- `format` — `text`, `markdown`, or `json`, defaulting to `markdown`
- `package` — optional repeated package ids used to restrict the report
- `output` — optional path for a JSON readiness artifact

## Prerequisites

`PublishReadiness` needs a release record from `CommitRelease` and any package-registry credentials or local tooling required to perform dry-run existence checks for the selected ecosystems.

## Side effects and outputs

The step is read-only. It may contact registries for existence checks, but it does not publish package artifacts.

When `output` is set, monochange writes a JSON readiness artifact that includes the release record commit, selected packages, package-set fingerprint, and publish input fingerprint. Re-run readiness if workspace configuration, manifests, lockfiles, or registry/tooling files change after the artifact was written.

## Example

```bash
mc step:publish-readiness --from HEAD
mc step:publish-readiness --from HEAD --output .monochange/local/readiness.json
mc step:publish-readiness --from v1.2.3 --package core --format json
```

A readiness-backed rate-limit plan can then consume the artifact:

```bash
mc step:plan-publish-rate-limits --mode publish --readiness .monochange/local/readiness.json
```
