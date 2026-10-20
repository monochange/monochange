---
monochange_app: fix
---

# Send a User-Agent when the app calls the GitHub API

Hosted release commits failed with `502` and "failed to mint installation token" because `POST /app/installations/{id}/access_tokens` went out without a `User-Agent` header, which GitHub rejects with `403 Forbidden`. The shared `reqwest::Client` built in `AppState::new` now sets `User-Agent: monochange`, so installation token, tree, commit, ref and pull request calls all identify the app. No configuration change is needed; redeploy monochange.dev to pick it up.
