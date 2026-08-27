---
monochange:
  bump: patch
  type: docs
monochange_core: docs
monochange_linting: docs
monochange_schema: docs
"@monochange/cli": docs
"@monochange/skill":
  bump: patch
  type: docs
---

# refresh documentation and remove stale CLI references

Updated the guide, reference, crate readme, and agent-facing documentation to match the current CLI model:

- configured `[cli.*]` workflows are documented as `monochange run <name>` commands
- the removed `mc` binary alias is no longer referenced as an install option
- stale `monochange <command>` invocations were replaced with the nested `step` or `run` paths
- generated subagent instructions no longer list the `monochange` executable twice
- knope migration guide now shows the built-in regex versioned-file support instead of a manual `sed` fallback
- duplicated `monochange --help` lines and a duplicated registry entry were removed
- prose was tightened and em dashes were replaced with colons or sentence breaks

The monochange skill now prefers the inline changeset type shorthand whenever the intended bump matches the type's default bump, and reserves object syntax for overriding a type's default bump.
