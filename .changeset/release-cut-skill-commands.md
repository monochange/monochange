---
"@monochange/skill": patch
---

# List the release cut sync steps in the skill command inventory

Agents that follow the packaged skill can now find the frozen release cut sync commands in its generated built-in command inventory. `monochange step sync-release` and `monochange step open-sync-request` were missing from that list even though the binary exposes them, so an agent checking which steps exist could mistake post-release sync for a repository-defined workflow command or a command that was removed. Both now appear with the other `CliStepDefinition` commands:

```text
- `step open-sync-request`
- `step sync-release`
```

This is a documentation-only patch. The inventory remains generated from the CLI command surface by `cargo xtask skill commands update`, so the packaged list matches the binary agents call.
