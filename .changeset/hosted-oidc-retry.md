---
monochange: fix
---

# Retry GitHub's OIDC endpoint before failing a hosted release commit

A hosted `CommitRelease` asks GitHub Actions for the job's OIDC token before calling monochange.dev. When that endpoint (`ACTIONS_ID_TOKEN_REQUEST_URL`) briefly returned a 5xx status, the release failed at once and blamed the workspace:

```text
error[config.invalid]: hosted CommitRelease OIDC token request failed with status 503 Service Unavailable
  help:    Check `monochange.toml` and the command arguments, then rerun the command.
```

The token request now retries connection failures and 5xx statuses after 2, 5, and 10 seconds, as `getIDToken` in `@actions/core` does. If GitHub still cannot issue the token, the error names the attempts and says it is a GitHub outage to rerun, and a 4xx status points at the job's `permissions: id-token: write`.

Failed requests to GitHub's OIDC endpoint or to the hosted backend now report `error[hosted.request_failed]` instead of `error[config.invalid]`, so they no longer suggest checking `monochange.toml`. A CI step that matched `config.invalid` for these failures should match `hosted.request_failed`. No configuration change is needed.
