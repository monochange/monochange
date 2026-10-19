---
monochange_app: breaking
---

# Share accepted feedback as screened contributor tasks

Projects can opt into the experimental shared compute flow and share an accepted item as a task that contributors run with their own agents. The new `monochange_app_feedback::tasks` module holds the task model (`TaskDraft`, `TaskTarget`, `ContributorTask`, `PublicTask`, `TaskStatus`) and `screen_task`, the rules that run before a task lists. `FeedbackService` gains `set_shared_compute`, `task_repository`, `share_task`, `withdraw_task`, `open_tasks`, and `open_task`, and `FeedbackState` stores `shared_compute` and `tasks` (both default when an older document loads).

`monochange_app_api::feedback::share_task` pins a task to the tip of the issue repository's default branch through the new `github_app::default_branch_head`, and lists it for `TASK_LIFETIME` (14 days). Anyone can read open tasks without signing in:

```sh
curl https://monochange.dev/api/projects/acme/invoices/tasks
curl https://monochange.dev/api/projects/acme/invoices/tasks/task-1
```

The console calls the new `set_shared_compute`, `share_task`, and `withdraw_task` server functions.

**Breaking change:** `ConsoleView` gains `shared_compute`, and `ConsoleItem` gains `tasks` and `shareable`, so code that builds them with struct literals must set the new fields:

```rust
ConsoleItem {
	// ...existing fields
	tasks: Vec::new(),
	shareable: false,
}
```
