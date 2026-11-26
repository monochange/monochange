---
monochange_publish: major
---

# Plan which packages a publish run may upload from its git ref

> **Breaking change:** `PublishRequest`, `PackagePublishOutcome`, and `PackagePublishSummary` gained fields, `PackagePublishStatus` gained two variants, and `pub_dev_trusted_publishing_ref_error` is removed. See [the 0.23 migration guide](https://monochange.github.io/monochange/guide/migrations/0.23.html).

The executor now defers a trusted pub.dev package to its own release tag instead of failing the run, and a run on such a package's own tag skips packages that publish in another run. Callers building these types with every field, or matching `PackagePublishStatus` exhaustively, must handle:

- `PublishRequest::release_tag: Option<PublishReleaseTag>`, the tag the package's release creates. `build_release_requests` fills it in; `None` means no tag.
- `PublishRequest::run_scope: PublishRunScope`, which the executor reads in release mode. `build_release_requests` sets `PublishRunScope::Publish`.
- `PackagePublishOutcome::deferred_to: Option<PublishDeferral>`, with the `reason` (`PublishDeferralReason`), `tag`, `ref`, and `command` of the run that publishes a deferred package.
- `PackagePublishSummary::deferred` and `PackagePublishSummary::out_of_scope`, omitted from JSON when zero, and `PackagePublishSummary::this_run`.
- `PackagePublishStatus::DeferredToTag` and `PackagePublishStatus::SkippedOutOfScope`. Neither fails the run, and `--resume` plans both again.

```rust
use monochange_publish::PublishRunScope;
use monochange_publish::assign_publish_run_scopes;
use monochange_publish::configured_release_tags;
use monochange_publish::current_env_map;

// before: monochange_publish::pub_dev_trusted_publishing_ref_error(&request, &env)
// after: plan the whole release against its tags, then read the scope of
// each selected request
let release_tags = configured_release_tags(&configuration, &release_requests);
assign_publish_run_scopes(&mut requests, &release_requests, &release_tags, &current_env_map());
for request in &requests {
    match &request.run_scope {
        PublishRunScope::Publish => {}
        PublishRunScope::DeferredToTag { deferral, .. } => println!("{}: {}", request.package_id, deferral.command),
        PublishRunScope::OutOfScope { message } => println!("{message}"),
        PublishRunScope::MissingReleaseTag => {}
    }
}
```

`missing_release_tag_errors(&configuration, discover_packages)` lists the trusted pub.dev packages that need `tag = true`, and calls `discover_packages` only when the configuration has a candidate. `publish_is_tag_bound(&request, &env)` reports whether a request can only publish from its tag.
