---
monochange:
  bump: patch
  type: docs
monochange_core:
  bump: none
  type: docs
monochange_linting:
  bump: none
  type: docs
monochange_schema:
  bump: none
  type: docs
"@monochange/cli":
  bump: none
  type: docs
"@monochange/skill":
  bump: none
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
