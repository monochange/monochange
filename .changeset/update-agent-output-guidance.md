---
"@monochange/skill": patch
---

# Teach agents to choose structured output explicitly

The monochange skill examples now use text for human-facing command defaults and request JSON or Markdown only when the workflow needs that artifact format. The guidance also treats `--quiet` and `--dry-run` as independent choices, matching the CLI contract.
