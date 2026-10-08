---
"monochange": minor
"monochange_core": minor
"monochange_config": minor
"monochange_publish": minor
"monochange_schema": patch
"@monochange/skill": patch
---

# Add npm staged publishing with `publish.flow`

npm packages can now defer the moment a release goes live. Set `flow = "staged"` and release publishes run `npm stage publish` instead of `npm publish`, so a version waits in the npm staging queue until a maintainer approves it with 2FA (`npm stage approve <stage-id>` or the Staged Packages tab on npmjs.com). Staging needs no 2FA, so CI stays unattended while every release gains a human approval gate.

```toml
# Before: every release publish goes live immediately.
[ecosystems.npm.publish]
trusted_publishing = true

# After: CI stages token-free; a maintainer approves each release with 2FA.
[ecosystems.npm.publish]
trusted_publishing = true
flow = "staged"

# Opt a single package back into immediate publishes.
[package.legacy.publish]
flow = "direct"
```

Details:

- `flow` accepts `"direct"` (default, current behavior) or `"staged"` on `[ecosystems.npm.publish]` and `[package.<id>.publish]`; package values override ecosystem defaults like every other publish option. Other ecosystems reject `"staged"` at config load.
- Staged publishing composes with trusted publishing: the trusted-publisher workflow stages through OIDC and approval requires interactive 2FA that CI cannot supply. This survives a compromised CI context, which a direct trusted publish does not.
- Successful staged publishes report a `staged` status (not `published`) with a `staged` summary count, and publish resume treats them as complete. Staged versions are invisible to the registry version probe, so re-running before approval stages again instead of skipping.
- Placeholder publishing always stays direct, matching the existing rule that placeholder publishing ignores publish modes; a placeholder must register the package immediately.
- Requires npm CLI 11.15+ and Node 22.14+; pnpm workspaces stage through `pnpm stage publish` (pnpm 11.3+).
- Release records now carry `flow` on each package publication target, so the release decision stays auditable. Older release records without the field parse as `direct`.
