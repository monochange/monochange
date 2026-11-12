---
monochange_app: fix
---

# Reduce repeated browser rendering code

The shared header's reactive child and project-card views erase their concrete render types at component boundaries, allowing browser bundles to share rendering machinery. The header retains its transition boundary so account controls remain visible while the session reloads during navigation. Deploy the server and both browser entries together so server rendering and hydration use matching view boundaries.
