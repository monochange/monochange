---
monochange: patch
---

# reduce binary size with dist profile and dependency cleanup

Add `[profile.dist]` for optimized release builds, remove `reqwest` `blocking` feature from production dependencies, and eliminate `default-features = true` overrides that pulled in unnecessary transitive features. Also add binary-size tracking CI job for pull requests.
