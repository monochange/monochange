# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## @monochange/feedback-worker [0.2.0](https://github.com/monochange/monochange/releases/tag/@monochange/feedback-worker/v0.2.0) (2026-11-08)

### 💥 Breaking Change

#### Run a connected feedback agent locally or on your own server

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #860](https://github.com/monochange/monochange/pull/860)

The first release adds `monochange-feedback-worker pair CONFIG.json` and `monochange-feedback-worker run CONFIG.json`. Pair through the project's feedback console using a single-use code, map permitted repositories to local checkouts, and configure the model endpoint and a preinstalled Docker or Podman check image.

```sh
monochange-feedback-worker pair .monochange/local/worker.json
monochange-feedback-worker run .monochange/local/worker.json
```

Planning reads a pinned export of the selected repository. After maintainer approval, implementation may change only the approved files, runs operator-configured checks in a container with no network or host credentials, and returns text changes for a draft pull request. Model credentials stay in the worker's process environment. Results are saved locally before submission so reconnecting can reconcile an interrupted response without repeating the agent run.

The worker has no merge or publishing operation. Keep its configuration and pending result file under `.monochange/local/` and revoke its credential in the console when disconnecting the machine.
