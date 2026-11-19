---
monochange_app: fix
---

# Document the feedback protocol's RPC operations

The feedback widget guide's host-backend example now issues client sessions through `POST /api/feedback_sdk_session?project={project}` with `{"project", "input": {"user_id", "surface"}}`, matching the `@monochange/feedback` example. The REST `POST /api/v1/feedback/{project}/sessions` route still works.
