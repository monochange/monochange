# Connected feedback agent

Install `@monochange/feedback-worker` on Node 22+ to connect a local or self-hosted agent to a monochange project. Git is required; implementation checks use a preinstalled Docker or Podman image.

Follow the [worker setup and approval guide](https://github.com/monochange/monochange/blob/main/docs/src/guide/feedback-connected-agents.md).

```sh
monochange-feedback-worker pair .monochange/local/feedback-worker.json
monochange-feedback-worker run .monochange/local/feedback-worker.json
```

The maintainer grants a repository scope during pairing. The worker exports a pinned commit without git credentials or local changes, holds model credentials outside agent tools, limits writes to approved paths, and runs configured checks in a container with no network. It returns plans for approval and implementation results for draft PRs. It never merges or publishes.

Keep the worker config and adjacent `.pending.json` result file private. Pending results are reconciled before new jobs are claimed. `run CONFIG.json --once` processes one available job or one pending result.
