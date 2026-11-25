---
monochange: minor
---

# Publish trusted pub.dev packages from their own release tag

A release that ships a trusted pub.dev package alongside packages for other registries now publishes cleanly. pub.dev only accepts an upload from a GitHub Actions run on the package's own tag (`refs/tags/<package tag>`), so `monochange publish packages` compares the run's `GITHUB_REF` with that ref before uploading:

- **On another ref**, the package is not uploaded and is marked `deferred_to_tag`. The run does not fail, and the packages after it still publish. Earlier versions attempted the upload, pub.dev rejected it, and every later package was skipped.
- **On the package's own tag**, only the packages released under that tag publish. The rest are marked `skipped_out_of_scope`, are not looked up on their registries, and do not count against the run's rate limits, so a rerun on that tag never republishes to other registries.

The report lists each deferred package with its tag and the command that publishes it:

```text
Would publish (1)
  web  1.1.0  npm

Deferred to release tags (1)
  app_dart  0.2.1  pub_dev  app_dart/v0.2.1

The registry only accepts these packages from a run on their release tag. Publish each one from its tag:
  gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1
```

With `--format json` and in the `--output` artifact, the package row carries the same details, and `summary.deferred` and `summary.out_of_scope` count the new statuses:

```json
{
	"package": "app_dart",
	"status": "deferred_to_tag",
	"deferred_to": {
		"tag": "app_dart/v0.2.1",
		"ref": "refs/tags/app_dart/v0.2.1",
		"command": "gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1"
	}
}
```

`monochange publish readiness` reports both statuses as `ready`. To publish a deferred package, run the publish workflow on the release's primary tag, then dispatch it on each `deferred_to.tag`, with publishing from `workflow_dispatch` events enabled on the package's pub.dev admin page.
