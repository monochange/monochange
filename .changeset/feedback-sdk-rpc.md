---
monochange_app: feat
---

# Serve the embedded feedback protocol as leptoskit RPC

The feedback SDK, connected-worker, and hosted-frame calls are now leptoskit RPC operations, published with `#[api]` and JSON Schema beside the website's other server functions. Each is a `POST /api/<operation>?project={project}` whose named JSON arguments also carry `project`:

| Operation                           | Arguments                                 | Replaces                                        |
| ----------------------------------- | ----------------------------------------- | ----------------------------------------------- |
| `feedback_sdk_session`              | `project`, `input: {user_id, surface}`    | `POST /api/v1/feedback/{project}/sessions`      |
| `feedback_sdk_suggestions`          | `project`, `input: {text}`                | `POST …/suggestions`                            |
| `feedback_sdk_create_conversation`  | `project`, `input: {kind, message, ...}`  | `POST …/conversations`                          |
| `feedback_sdk_conversation`         | `project`, `id`                           | `GET …/conversations/{id}`                      |
| `feedback_sdk_conversation_message` | `project`, `id`, `input: {body, version}` | `POST …/conversations/{id}/messages`            |
| `feedback_sdk_submit`               | `project`, `input: {conversation, ...}`   | `POST …/items`                                  |
| `feedback_sdk_item`                 | `project`, `id`                           | `GET …/items/{id}`                              |
| `feedback_sdk_reply`                | `project`, `id`, `input: {body}`          | `POST …/items/{id}/messages`                    |
| `feedback_sdk_vote`                 | `project`, `id`, `voted`                  | `PUT` / `DELETE …/items/{id}/vote`              |
| `feedback_worker_pair`              | `project`                                 | `POST …/workers/pair`                           |
| `feedback_worker_claim`             | `project`                                 | `POST …/workers/claim`                          |
| `feedback_worker_heartbeat`         | `project`, `id`, `input`                  | `POST …/jobs/{id}/heartbeat`                    |
| `feedback_worker_result`            | `project`, `id`, `input`                  | `POST …/jobs/{id}/result`                       |
| `feedback_iframe_session`           | `input: {project, ...}`                   | `POST /api/feedback/iframe-session`             |

A new `sdk_gate` middleware gives the cross-origin operations the same CORS policy as the REST routes: the `?project=` query names the project whose registered origins are allowed, because a browser preflight carries no body, and an operation rejects arguments that name a different project with `400 invalid_input`. Responses are `no-store`, quota failures keep `429` with `Retry-After`, and failures keep their REST statuses with a `{"status", "code", "message"}` body. The bearer-session gate no longer treats integration, client, or worker tokens on these paths as website sessions.

`monochange_app_api::embedded_api::rpc` exposes each call over a request's headers, so the RPC and REST transports run the same handlers, and the REST routes stay as aliases for SDKs released before the RPC operations. The embedded inputs (`SessionInput`, `SuggestInput`, `DraftInput`, `MessageInput`, `SubmitInput`, `ReplyInput`, `Heartbeat`, `WorkerResult`, and the frame's `SessionInput`) are now public with `Serialize` and `schemars::JsonSchema`, and `embedded_api::cors_for_project` applies a project's origin policy without a path.
