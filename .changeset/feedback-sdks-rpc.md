---
"@monochange/feedback": major
"@monochange/feedback-worker": feat
monochange_feedback: feat
---

# Call the feedback protocol's leptoskit RPC operations

`FeedbackClient` (npm and Flutter) and the connected worker now call the feedback protocol's RPC operations, `POST {endpoint}/api/<operation>?project={project}` with named JSON arguments, instead of the REST routes under `/api/v1/feedback/{project}`:

| Call                                  | Operation                                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `suggest`                             | `feedback_sdk_suggestions` with `{project, input: {text}}`                                             |
| `startConversation`                   | `feedback_sdk_create_conversation` with `{project, input: {message, kind, context}}`                   |
| `getConversation`                     | `feedback_sdk_conversation` with `{project, id}`                                                       |
| `sendMessage`                         | `feedback_sdk_conversation_message` with `{project, id, input: {body, version}}`                       |
| `submit`                              | `feedback_sdk_submit` with `{project, input: {conversation, version}}` and the same key                |
| `getItem`                             | `feedback_sdk_item` with `{project, id}`                                                               |
| `reply`                               | `feedback_sdk_reply` with `{project, id, input: {body}}`                                               |
| `vote`                                | `feedback_sdk_vote` with `{project, id, voted}`                                                        |
| worker pair, claim, heartbeat, result | `feedback_worker_pair`, `feedback_worker_claim`, `feedback_worker_heartbeat`, `feedback_worker_result` |

Errors keep their `code`, `status`, and `Retry-After`. When a site answers an operation with a `404` that carries no `code`, it predates the RPC operations, so the client switches to the REST routes for the rest of its life, and the worker for the rest of its process. No host code changes are needed. The hosted sign-in page now calls `feedback_iframe_session`, and the host-backend example issues sessions with `feedback_sdk_session` (`{project, input: {user_id, surface}}`); the REST `POST /api/v1/feedback/{project}/sessions` keeps working.

`FeedbackClient` (npm) gains a public `origin` field beside `base`; its method signatures are unchanged. The API classifier treats any change to an exported class as modified, so `@monochange/feedback` takes a major bump.
