---
monochange_app: fix
---

# Split the projects landing route

The projects landing page and organization details now load separate browser chunks. This reduces each page's route transfer at the cost of additional shared code in the complete bundle.
