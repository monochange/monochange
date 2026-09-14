---
"monochange": patch
---

# Update rustls to clear RUSTSEC-2026-0285

`cargo deny check` began failing with a newly published advisory against the HTTP client stack:

```text
error[vulnerability]: TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries
    ID: RUSTSEC-2026-0285
    Solution: Upgrade to >=0.23.45
```

`rustls` is reached through `hyper-rustls` and `octocrab`, which monochange uses for source-provider API calls. The lockfile now pins the patched release. No monochange code changes; this is a dependency update only.
