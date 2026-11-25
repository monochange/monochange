---
monochange: minor
---

# Publish trusted pub.dev packages from their own release tag

A release that ships a trusted pub.dev package alongside packages for other registries now publishes cleanly. pub.dev only accepts an upload from a GitHub Actions run that a `push` or `workflow_dispatch` event started on the package's own tag (`refs/tags/<package tag>`). `monochange publish packages` checks the run's `GITHUB_REF`, `GITHUB_REF_TYPE`, and `GITHUB_EVENT_NAME` against that rule before uploading a pub.dev package that publishes with trusted publishing and no `PUB_TOKEN`:

- **When the run does not match**, the package is not uploaded and is marked `deferred_to_tag`. The run does not fail, and the packages after it still publish. Earlier versions attempted the upload, pub.dev rejected it, and every later package was skipped.
- **On a pub.dev package's own tag**, only the trusted pub.dev packages released under that tag publish. The rest are marked `skipped_out_of_scope`, are not looked up on their registries, and do not count against the run's rate limits, so a rerun on that tag never republishes to other registries. Each message names the run that publishes the package.
- **On the release's primary tag, on a branch, or on any other tag**, every other package publishes as before. A workspace without trusted pub.dev packages publishes exactly as before on every ref, and so does a release whose only tags belong to trusted pub.dev packages.

An empty `PUB_TOKEN` counts as no token, so the package still waits for its tag. The report lists each deferred package with its tag and the command that publishes it:

```text
Would publish (1)
  web  1.1.0  npm

Deferred to release tags (1)
  app_dart  0.2.1  pub_dev  app_dart/v0.2.1

The registry only accepts these packages from a `push` or `workflow_dispatch` run on their release tag. Publish each one from its tag:
  gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1
```

When the run is on the right tag but a `release` or `create` event started it, or the run has no `GITHUB_REF`, the package is still deferred and its message says what to change in the workflow. With `--format json`, the package row carries the reason and the same details, and `summary.deferred` and `summary.out_of_scope` count the new statuses:

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

`reason` is `wrong_ref`, `wrong_ref_type`, `wrong_event`, or `unknown_ref`. The command dispatches the package's `publish.trusted_publishing.workflow` when it sets one. `monochange publish readiness` checks every package in the release whichever ref it runs on. To publish a deferred package, run the publish workflow on the release's primary tag, then dispatch it on each `deferred_to.tag`, with publishing from `workflow_dispatch` events enabled on the package's pub.dev admin page.
