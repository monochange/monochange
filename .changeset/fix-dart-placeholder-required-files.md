---
monochange_publish: patch
---

# Include required files in placeholder publish directories

Placeholder publish directories now include a `LICENSE` and `CHANGELOG.md` alongside the placeholder `README.md` and registry manifest. This lets Dart placeholder packages pass pub.dev's required-file validation during `mc step:placeholder-publish`.
