---
monochange_app: fix
---

# Preserve workflows in hosted release commits

Hosted release commits now compare the final `.github/workflows` subtree with the prepared base for both uploaded files and prebuilt trees. Requests fail if workflow contents change, a protected directory becomes a file, or a tree listing is incomplete. Uploaded file paths must be canonical repository-relative paths and cannot replace or delete `.github` or `.github/workflows`; uploads to other `.github` files remain supported.
