---
monochange_app: fix
---

# Generate WOFF2 fonts during web builds

The web asset plugin now requires `pyftsubset` with Brotli support. It generates deployable WOFF2 subsets from `app/public/fonts` and removes the copied TTFs from the assembled site. The devenv shell, CI, and Docker builder provide the required tool.
