---
monochange_app: patch
---

# Explain how trusted pub.dev packages publish from their own tag

The trusted-publishing, CI, and multi-package publishing guides now describe pub.dev's ref rule, the `deferred_to_tag` and `skipped_out_of_scope` statuses, the pub.dev admin settings (repository, tag pattern, `workflow_dispatch` events, optional environment), and a workflow that publishes on the primary tag and then dispatches each deferred tag. The `PublishPackages` and `PublishReadiness` references cover the same behaviour, and the 0.23 migration guide lists the configuration, JSON, and Rust updates.
