---
monochange_app: website_fix
---

# Explain how to recover rejected release pull request updates

When a hosted release workflow fails to refresh an existing release pull request with HTTP 422 because the pull request was not created by this deployment's verified flow, monochange.dev now tells you why it was rejected and to close that pull request once and re-run the release workflow so monochange opens a verified one.
