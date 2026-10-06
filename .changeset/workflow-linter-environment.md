---
monochange: none
---

# Use the workflow linter supplied by devenv

`lint:workflows` and `fix:workflows` use the declared devenv tool instead of overriding it with a potentially older personal installation. This keeps validation compatible with GitHub's self-repository workflow and action references.
