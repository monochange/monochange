---
monochange_app: fix
---

# Resolve production deployment credentials in the environment-bound job

The website deployment job now runs directly in `ci.yml` with the `website-production` environment, so its SSH key reaches the deployment step without crossing a reusable-workflow secret boundary. The job validates key presence and parsing before building the update and removes the temporary key after use. Operators receive an early error when deployment credentials are missing or malformed.
