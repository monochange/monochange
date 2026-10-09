---
monochange_app: feat
---

# Store project feedback and expose the console and portal

Adds the `monochange_app_feedback` crate: the feedback pipeline with intake validation, prompt-injection screening, a `TriageEngine` seam with `RuleBasedTriage`, discussion and re-triage, voting, maintainer-only decisions, duplicate folding, and notifications. It also includes the disclosure rules (`Sensitivity::allowed_on(Surface, RepositoryVisibility)`) and the GitHub handoff drafts. `FeedbackService::from_state` and `state` round-trip a project's `FeedbackState`. Issue and pull request references now carry their repository, so merges and releases match by repository as well as number (`observe_merge(repository, number)` and `ship_merged`).

Database migration `005_create_project_feedback` adds `project_feedback`, one versioned JSON document per project, and `feedback_notifications`. `monochange_app_db::feedback::save_feedback` uses optimistic concurrency, and `monochange_app_api::feedback::update` reruns an operation on a lost race, up to three attempts. `monochange_app_api::feedback::create_issue` opens an issue through the GitHub App installation token with the new `github_app::create_issue`, then links it.

New server functions: `feedback_console`, `feedback_action`, and `maintainer_reply` for maintainers, plus `portal_view`, `share_feedback`, `portal_vote`, `portal_reply`, `similar_requests`, and `preview_markdown` for the public portal. Visitors are identified by a random `__Host-monochange_viewer` cookie, mapped to a per-project pseudonymous id with `feedback::viewer_id`. A project with a private or disconnected repository is treated as private.
