---
monochange: change
---

# Call the monochange app's hosted release operations over RPC

Hosted `CommitRelease`, `OpenReleaseRequest`, and `monochange gh workflow run` now call the monochange app's leptoskit RPC operations: `POST /api/hosted_release_commit`, `/api/hosted_release_request`, and `/api/hosted_workflow_dispatch`, with the request sent as `{"request": ...}`. Failure messages keep their `hosted CommitRelease failed with HTTP 409 Conflict: ...` shape, and now show the app's `message` instead of the raw JSON body, so the `monochange/actions` hosted-release hints still match.

Deployments that predate the RPC operations, including self-hosted apps set through `MONOCHANGE_HOSTED_URL`, keep working: when the RPC route answers `404` without an RPC error body, the CLI sends the same request to the REST route (`/api/release-commits`, `/api/release-requests`, `/api/workflow-dispatches`). A `404` that carries an RPC error, such as a repository without the monochange GitHub App, is reported as is.

A request that cannot reach the app now reads `hosted OpenReleaseRequest request failed: ...` instead of `hosted OpenReleaseRequest failed: ...`, matching the other hosted operations.
