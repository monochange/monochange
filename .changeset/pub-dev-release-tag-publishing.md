---
monochange: minor
---

# Publish trusted pub.dev packages from their own release tag

A release that ships a trusted pub.dev package alongside packages for other registries now publishes cleanly. pub.dev only accepts an upload from a GitHub Actions run that a `push` or `workflow_dispatch` event started on the package's own tag (`refs/tags/<package tag>`). `monochange publish packages` checks the run's `GITHUB_REF`, `GITHUB_REF_TYPE`, and `GITHUB_EVENT_NAME` against that rule before uploading a pub.dev package that publishes with trusted publishing and no `PUB_TOKEN`:

- **When the run does not match**, the package is not uploaded and is marked `deferred_to_tag`. The run does not fail, and the packages after it still publish. Earlier versions attempted the upload, pub.dev rejected it, and every later package was skipped.
- **On the release's tags**, each package of the release publishes in exactly one run. A trusted pub.dev package publishes in the run on its own tag; every other package in the run on the primary tag, or without one, on its own tag or the first tag that no trusted pub.dev package uses. The runs on the release's other tags mark the package `skipped_out_of_scope`: it is not looked up on its registry and does not count against the run's rate limits, and its message names the run that publishes it. A run on a non-primary package tag, such as `web/v1.1.0`, therefore no longer publishes the release's other packages. `--package`, `--group`, and `--ecosystem` select from that plan and never move a package into another run.
- **On a branch or on a tag the release does not create**, every package but the trusted pub.dev packages publishes as before. A workspace without trusted pub.dev packages publishes exactly as before on every ref.

An empty `PUB_TOKEN` counts as no token, so the package still waits for its tag. The report lists each deferred package with its tag and the command that publishes it:

```text
Would publish (1)
  web  1.1.0  npm

Deferred to release tags (1)
  app_dart  0.2.1  pub_dev  app_dart/v0.2.1

The registry only accepts these packages from a `push` or `workflow_dispatch` run on their release tag. Publish each one from its tag:
  gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1
```

When the run is on the right tag but a `release` or `create` event started it, or the run has no `GITHUB_REF`, the package is still deferred and its message says what to change in the workflow. No run of that workflow can publish it, so the run ends with a warning that counts the packages left unpublished until the workflow's trigger or `GITHUB_REF` is fixed. With `--format json`, the package row carries the reason and the same details, and `summary.deferred` and `summary.out_of_scope` count the new statuses:

```json
{
	"package": "app_dart",
	"status": "deferred_to_tag",
	"deferred_to": {
		"reason": "wrong_ref",
		"tag": "app_dart/v0.2.1",
		"ref": "refs/tags/app_dart/v0.2.1",
		"command": "gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1"
	}
}
```

`reason` is `wrong_ref`, `wrong_ref_type`, `wrong_event`, or `unknown_ref`. The command dispatches the package's `publish.trusted_publishing.workflow` when it sets one, and is omitted when monochange cannot determine the workflow. `monochange publish readiness` checks every package in the release whichever ref it runs on. To publish every package, start one run of the publish workflow for each tag the release creates, from tag pushes or by dispatching it on each tag, and let the run on the primary tag (or, without one, on the first tag no trusted pub.dev package uses) dispatch each `deferred_to.command` it reports. Enable publishing from `push` and `workflow_dispatch` events on the package's pub.dev admin page.
