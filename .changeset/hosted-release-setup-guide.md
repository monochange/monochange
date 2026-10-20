---
monochange_app: website_fix
---

# Set up bot release pull requests without a token

The GitHub App guide now walks through using monochange.dev to create release commits and release pull requests: why the app replaces `GITHUB_TOKEN` and personal access tokens, what to approve when installing the app, the `monochange.toml` change, and a complete GitHub Actions job that only needs `id-token: write`. It no longer says hosted releases need a separate rollout, and it explains that monochange.dev does not issue per-user `MONOCHANGE_TOKEN` values, so hosted release commits run from GitHub Actions.
