---
monochange_app: feat
---

# Build accepted feedback with the repository's agent

The maintainer console offers `ConsoleAction::BuildWithAgent` (`"build_with_agent"`) on accepted items whose GitHub issue names its repository. `monochange_app_api::feedback::start_agent_build(state, scope, item_id)` checks that the issue's repository has `AGENT_WORKFLOW` (`.github/workflows/monochange-feedback-build.yml`), sends it a `repository_dispatch` event named `AGENT_BUILD_EVENT` (`monochange-feedback-build`), and moves the item to building. A missing workflow returns the new `FeedbackError::AgentWorkflowMissing`, and a failed dispatch leaves the item accepted so it can be retried.

The event's `client_payload` is the new `monochange_app_feedback::BuildRequest`, ids only:

```json
{
	"item": "fb-12",
	"repository": "acme/web",
	"issue_number": 42,
	"branch": "fix/feedback-fb-12"
}
```

`github_app::repository_file_exists` and `github_app::dispatch_repository_event` are the new GitHub calls behind it; both use the Contents access hosted releases already need. The repository's workflow, which runs the agent with a read-only token and opens a draft pull request with the `Feedback-Item:` trailer from a separate job, is documented in the new "Build accepted feedback with an agent" guide.
