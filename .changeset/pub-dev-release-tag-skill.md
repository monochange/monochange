---
"@monochange/skill": patch
---

# Describe how trusted pub.dev packages publish from their own tag

The trusted-publishing guide in the skill no longer says that `monochange publish packages` fails a run whose ref is not a tag. It now tells agents that a trusted pub.dev package the run cannot upload is marked `deferred_to_tag` with a `reason` and a `gh workflow run` command. It also explains that each package of a release publishes in exactly one run on the release's tags, so the publish workflow must start a run for every release tag, and that a package publishing with a `PUB_TOKEN` needs `publish.trusted_publishing = false` to pass `monochange step validate`.
