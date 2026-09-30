---
"@monochange/skill": patch
"@monochange/cli": patch
---

# Guide agents through verified adoption and release workflows

The skill now distinguishes built-in commands from configured workflows, explains incremental package ownership and discovery filters, and documents the supported version-file, prerelease, stream, and release-note options. Agents can follow existing authorization through local preparation without repeatedly asking for permission.

For a repository with no custom workflows, use `monochange create`, `monochange preview`, and `monochange prepare`; `monochange run <name>` requires a matching `[cli.<name>]` configuration. Poetry guidance uses the current `poetry lock` command, and Deno guidance covers JSONC manifests.

The updated guidance is verified through CLI contracts and agent tasks across all six supported ecosystems.
