---
monochange: feat
monochange_core: major
monochange_config: feat
monochange_schema: fix
"@monochange/skill":
  bump: patch
  type: docs
---

# Allow local publishing alongside trusted publishing

`publish.trusted_publishing` gains a `mode` setting so one configuration can support both publishing paths instead of forcing a choice between trusted publishing and local publishing. `mode = "required"` keeps the existing strict behavior: a verifiable CI/OIDC identity must be present and match the configured repository, workflow, and environment. `mode = "preferred"` verifies that same context whenever a CI identity is detected, and otherwise falls back to local credentials instead of failing before any registry mutation.

```toml
[ecosystems.dart.publish.trusted_publishing]
enabled = true
mode = "preferred"
repository = "acme/widgets"
workflow = "publish.yml"
environment = "publisher"
```

Use `preferred` for repositories that publish with OIDC from CI but also let maintainers run `monochange run publish` locally with their own registry credentials. The mode only relaxes the identity requirement: CI context mismatches still fail, and `enabled = false` remains the explicit opt-out from trusted publishing entirely.
