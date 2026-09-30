---
monochange_analysis: patch
---

# Preserve native manifest controls in analysis snapshots

Retain `go.mod`, `pyproject.toml`, and GitHub Actions control manifests when materializing Git revisions and staged snapshots. Workspaces with configured Go or Python packages can now pass configuration validation during analysis of supported packages. This preserves package controls without adding Go, Python, or GitHub Actions semantic analyzers.
