---
"monochange_app": website_fix
---

# Fail hosted release commits that GitHub does not verify

The monochange app now stops a hosted release when GitHub does not verify the release commit, and leaves the release branch where it was. Before this fix, the release branch moved to the unverified commit anyway, so a repository with required signed commits only learned of the problem when branch protection blocked the merge. The error names the commit and the reason GitHub gave.

This changes no settings or configuration. Repositories with verified-commit requirements now get an explicit failure instead of a misleading success.
