---
app: fix
---

# Validate deployment credentials before building

The deployment job rejects missing or malformed SSH keys before building an application image. Operators can correct the production environment credentials without waiting for a build to finish.
