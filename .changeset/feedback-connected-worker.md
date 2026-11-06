---
"@monochange/feedback-worker": major
---

# Run a connected feedback agent locally or on your own server

The first release adds `monochange-feedback-worker pair CONFIG.json` and `monochange-feedback-worker run CONFIG.json`. Pair through the project's feedback console using a single-use code, map permitted repositories to local checkouts, and configure the model endpoint and a preinstalled Docker or Podman check image.

```sh
monochange-feedback-worker pair .monochange/local/worker.json
monochange-feedback-worker run .monochange/local/worker.json
```

Planning reads a pinned export of the selected repository. After maintainer approval, implementation may change only the approved files, runs operator-configured checks in a container with no network or host credentials, and returns text changes for a draft pull request. Model credentials stay in the worker's process environment. Results are saved locally before submission so reconnecting can reconcile an interrupted response without repeating the agent run.

The worker has no merge or publishing operation. Keep its configuration and pending result file under `.monochange/local/` and revoke its credential in the console when disconnecting the machine.
