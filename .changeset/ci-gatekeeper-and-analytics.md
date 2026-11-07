---
monochange: feat
---

# Add `monochange ci` for conditional CI gating, outcome verification, and repo-scoped analytics

`monochange ci` adds four subcommands to gate CI jobs, verify main-branch outcomes, and inspect decision accuracy with repository-scoped authentication:

- `monochange ci auth init|verify|token` derives a canonical repository identifier and issues or verifies an HMAC-SHA256 repo-scoped token stored in `.monochange/ci-auth.json` so only the local repository can read its CI history and analytics.
- `monochange ci decide` evaluates whether CI jobs should run (`y` or `n`) across `opencode-go` (`deepseek-v4.1`), `openrouter`, `cloudflare`, `jev`, `openai-compatible`, and deterministic `graph` fallback, supporting conditional `main` runs with `--max-skips-on-main`, unconditional overrides, and slow-job red-flag suggestions.
- `monochange ci verify` classifies post-run outcomes into `false_negative` (skipped when it should have run), `false_positive` (ran when unaffected), `true_negative`, and `true_positive`, automatically persisting learned override rules on false negatives.
- `monochange ci analytics` and `monochange ci history` require valid repo authentication and report false negatives, false positives, precision, recall, and runner-minutes saved.
