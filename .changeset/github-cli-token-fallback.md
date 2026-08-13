---
monochange_github: minor
---

# Add GitHub CLI token fallback

GitHub release and release-request automation now resolves an API token with the precedence `GITHUB_TOKEN` > `GH_TOKEN` > `gh auth token`. When neither environment variable is set, monochange retrieves the authenticated GitHub CLI credential via `gh auth token`, so local workflows that are already signed in with `gh auth login` no longer need a manual token variable.

The Git Database commit-verification client still requires `GITHUB_COMMIT_TOKEN` or `GITHUB_TOKEN` so GitHub Actions can keep auto-signing verified commits with the web-flow GPG key.
