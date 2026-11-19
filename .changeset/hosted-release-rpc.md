---
monochange_app: feat
monochange_core: major
---

# Serve hosted release operations as leptoskit RPC

The hosted release commit, release pull request, and workflow dispatch that the monochange CLI requests from CI are now leptoskit RPC operations, published with `#[api]` beside the website's other server functions:

| Operation                  | Route                                | Arguments                                    | Result                           |
| -------------------------- | ------------------------------------ | -------------------------------------------- | -------------------------------- |
| `hosted_release_commit`    | `POST /api/hosted_release_commit`    | `{"request": HostedCommitRequest}`           | `HostedCommitResponse`           |
| `hosted_release_request`   | `POST /api/hosted_release_request`   | `{"request": HostedReleaseRequest}`          | `HostedReleaseResponse`          |
| `hosted_workflow_dispatch` | `POST /api/hosted_workflow_dispatch` | `{"request": HostedWorkflowDispatchRequest}` | `HostedWorkflowDispatchResponse` |

A new `machine_gate` middleware verifies the workflow's GitHub Actions OIDC token or `MONOCHANGE_TOKEN` before the body is read, so an unauthenticated caller still cannot make the server buffer a release payload, and raises the body limit to each operation's own (8 MiB for release commits). Failures keep the REST statuses (`400`, `401`, `403`, `404`, `409`, `413`, `422`, `502`, `503`) with a `{"status": 409, "message": "..."}` JSON body.

`POST /api/release-commits`, `POST /api/release-requests`, and `POST /api/workflow-dispatches` stay as aliases for CLIs released before the RPC operations, and both transports run the new `monochange_app_api::release_api::release_commit`, `release_request`, and `monochange_app_api::workflow_api::workflow_dispatch` cores. `release_api::Caller` and `release_api::authenticate` are now public for the gate.

With the `schema` feature, `monochange_core` now derives `schemars::JsonSchema` for `HostedReleaseRequest`, `HostedReleaseResponse`, `SourceChangeRequest`, `SourceChangeRequestOperation`, `SourceChangeRequestBodyTruncation`, and `CommitMessage`, so the RPC specification describes every hosted operation's arguments and results. The derives are opt-in and change no serialized shape or default-feature API, but the API classifier treats an attribute change on an existing public type as a modified item, so `monochange_core` takes a major bump, which the pending `github-release-cap-settings` change already requires.
