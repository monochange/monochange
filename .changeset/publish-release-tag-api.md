---
monochange_publish: major
---

# Plan which packages a publish run may upload from its git ref

> **Breaking change:** `PublishRequest`, `PackagePublishOutcome`, and `PackagePublishSummary` gained fields, `PackagePublishStatus` gained two variants, and `pub_dev_trusted_publishing_ref_error` is removed. See [the 0.23 migration guide](https://monochange.github.io/monochange/guide/migrations/0.23.html).

The executor now defers a trusted pub.dev package to its own release tag instead of failing the run, and a run on that tag skips packages released under other tags. Callers building these types with every field, or matching `PackagePublishStatus` exhaustively, must handle:

- `PublishRequest::release_tag: Option<PublishReleaseTag>`, the tag the package's release creates. `build_release_requests` fills it in; `None` means no tag.
- `PackagePublishOutcome::deferred_to: Option<PublishTagRun>`, with the `tag`, `ref`, and `command` of the run that publishes a deferred package.
- `PackagePublishSummary::deferred` and `PackagePublishSummary::out_of_scope`.
- `PackagePublishStatus::DeferredToTag` and `PackagePublishStatus::SkippedOutOfScope`. Neither fails the run, and `--resume` treats `SkippedOutOfScope` as complete.

```rust
use monochange_publish::PublishRunScope;
use monochange_publish::current_env_map;
use monochange_publish::plan_publish_run_scopes;

// before: monochange_publish::pub_dev_trusted_publishing_ref_error(&request, &env)
// after: one scope per request, in request order
for (request, scope) in requests.iter().zip(plan_publish_run_scopes(&requests, &current_env_map())) {
    match scope {
        PublishRunScope::Publish => {}
        PublishRunScope::DeferredToTag(run) => println!("{}: {}", request.package_id, run.command),
        PublishRunScope::OutOfScope => {}
        PublishRunScope::MissingReleaseTag => {}
    }
}
```

`missing_release_tag_errors(&configuration)` lists the trusted pub.dev packages that need `tag = true`, and `publish_is_tag_bound(&request, &env)` reports whether a request can only publish from its tag.
