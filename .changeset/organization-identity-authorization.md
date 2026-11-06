---
monochange_app: fix
---

# Authorize organization access through stable provider identities

Bind dashboard organization and project authorization to each installation's stable provider account ID. GitHub membership responses must identify that same account before granting repository access, and legacy installations resolve their identity before membership checks. Organization logins remain display metadata; duplicate stored logins cannot select a tenant through public project routes. No configuration or data migration is required.
