---
monochange_core: minor
monochange_hosting: minor
monochange_github: patch
---

# Keep GitHub release notes and release PRs within body limits

Large release notes now retain metadata and whole entries up to GitHub's 125,000-character limit, reserving room for a link to the complete changelog at the release tag. Smaller notes remain unchanged. Entries that cannot fit are omitted whole. Non-Markdown formats fall back to the changelog link.

Release PR limits now also cover oversized target lists and changelog pointers. Those descriptions become a short continuation notice; the committed changelog files retain every detail.

No configuration is required. To choose smaller bodies:

```toml
[source.releases]
max_body_chars = 100000

[source.pull_requests]
max_body_chars = 60000
```

GitHub caps cannot be raised by configuration. Library callers can use `monochange_hosting::body_limit::bound_body(body, boundaries, notice, max_chars)` to retain complete blocks while budgeting for a continuation notice. `ProviderReleaseSettings` gains `max_body_chars: Option<usize>`; struct literals can use `..Default::default()` for the default cap.
