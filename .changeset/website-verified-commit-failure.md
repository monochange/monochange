---
"monochange_app": website_fix
---

# Fail hosted release commits that GitHub does not verify

The monochange app now fails a hosted release commit instead of falsely reporting it as verified when GitHub does not sign the commit. Before this fix, the endpoint returned `verified = true` regardless of GitHub's response, so a repository with required signed commits would only learn of the problem when the release branch protection blocked the merge.

This changes no settings or configuration. Repositories with verified-commit requirements now get an explicit failure instead of a misleading success.
