---
monochange: patch
---

# Resolve custom tag formats and prerelease baselines

Resolve tag-based version baselines, analysis release references, and previous release titles using the owner's complete `version_format`. Custom prefixes, suffixes, ecosystem names, repeated version variables, and prerelease identifiers such as `dev.7` now match their own release tags and ignore unrelated primary tags. Select the highest matching SemVer independently of Git's tag sort order.
