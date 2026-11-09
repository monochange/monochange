---
monochange_core: major
---

> **Breaking change:** Rust callers constructing every field of `ProviderReleaseSettings` must initialize the new `max_body_chars: Option<usize>` field. See [the 0.22 migration guide](https://monochange.github.io/monochange/guide/migrations/0.22.html).

# Configure smaller GitHub release bodies

Hosted releases default to GitHub's 125,000-character cap. Set `[source.releases].max_body_chars` to choose a smaller preview while preserving full changelog files.

Rust callers can add `max_body_chars: None` to existing complete struct literals, or use struct update syntax:

```rust
use monochange_core::ProviderReleaseSettings;

let settings = ProviderReleaseSettings {
    max_body_chars: Some(100_000),
    ..Default::default()
};
```

Existing TOML configurations and callers using `Default` need no changes.
