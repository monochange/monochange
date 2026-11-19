# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## @monochange/feedback [0.3.0](https://github.com/monochange/monochange/releases/tag/@monochange/feedback/v0.3.0) (2026-11-19)

### 💥 Breaking Change

#### Call the feedback protocol's leptoskit RPC operations

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #903](https://github.com/monochange/monochange/pull/903)

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

## @monochange/feedback [0.2.0](https://github.com/monochange/monochange/releases/tag/@monochange/feedback/v0.2.0) (2026-11-08)

### 💥 Breaking Change

#### Install feedback on any website

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #860](https://github.com/monochange/monochange/pull/860)

The first release provides `FeedbackClient` for headless integrations and `mountFeedback` for an accessible Shadow DOM widget. Users can discuss a request, find similar requests, upvote, submit, and follow delivery updates without a GitHub account.

```ts
import { mountFeedback } from "@monochange/feedback";

const feedback = mountFeedback({
	endpoint: "https://monochange.dev",
	project: 123,
	getClientToken: async () => (await fetch("/api/feedback-session")).json(),
	getContext: () => ({ route: location.pathname }),
});
```

Issue short-lived feedback sessions through your authenticated backend. Call `feedback.reset()` on logout or account changes. Backend credentials must never be embedded in client code. Submission retries keep the same idempotency key.

For a feedback page embedded inside an iframe, use `presentation: "inline"`. The form fills its container, omits the floating launcher and close button, and appears without a modal backdrop or automatic focus. Explicit `open()`, `close()`, and account-reset behavior remain available.
