---
monochange_app: patch
---

# Explain GitHub release body limits and Rust settings migration

The GitHub automation guide now describes how oversized release notes retain complete entries and metadata, how continuation links fit within the size budget, and how to lower the release and PR limits. The 0.22 migration guide shows Rust callers how to initialize `ProviderReleaseSettings::max_body_chars`; existing TOML configurations need no changes.
