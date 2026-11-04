---
monochange_app: feat
---

# Authenticate embedded feedback and approve connected-agent plans

Adds the project-scoped `/api/v1/feedback/{project}` API for backend-issued sessions, private draft conversations, suggestions, idempotent submissions, discussion, votes, worker pairing, leased jobs, and authenticated results. Register integrations in the feedback console, then exchange a backend credential at `POST /sessions` with the host-authenticated `user_id` and `surface` (`web` or `mobile`). Backend credentials are returned once; only digests persist. Rotating an integration expires its existing sessions.

Projects default to manual planning and can enable automatic planning for new eligible feature requests with a daily budget. Plans bind a material brief revision and pinned repository commit; only a maintainer can approve implementation. Connected workers receive scoped jobs and return changes for draft PRs. Existing direct GitHub build dispatch cannot bypass approval for projects using the new flow.

Set `OPENROUTER_API_KEY` and `MONOCHANGE_FEEDBACK_MODEL` to enable semantic suggestions and assistant replies. `MONOCHANGE_FEEDBACK_COMPLETIONS_URL` can select a deployment-owned compatible endpoint. Without a configured provider, or when it fails, keyword suggestions remain available and drafts are preserved. Existing project feedback documents load with empty integration and planning state; intake, quotas, idempotency, and jobs share the existing transactional save.

Fallback bug triage records suggested verification steps as unverified. Providing screen context alone no longer marks a bug as reproduced.
