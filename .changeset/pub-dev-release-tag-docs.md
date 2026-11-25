---
monochange_app: patch
---

# Explain how trusted pub.dev packages publish from their own tag

The trusted-publishing, CI, and multi-package publishing guides now describe pub.dev's ref and event rule, the `deferred_to_tag` and `skipped_out_of_scope` statuses with each deferral reason, the pub.dev admin settings (repository, tag pattern, `workflow_dispatch` events, optional environment), and a workflow that publishes on the primary tag and then dispatches each deferred tag. The `PublishPackages` and `PublishReadiness` references cover the same behaviour, the trusted-publishing agent skill matches it, and the 0.23 migration guide lists the configuration, JSON, and Rust updates.
