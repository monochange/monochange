# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## monochange_feedback [0.2.1](https://github.com/monochange/monochange/releases/tag/monochange_feedback/v0.2.1) (2026-11-25)

### 🚀 Feature

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

```text
# before: the REST route an older site serves
POST {endpoint}/api/v1/feedback/42/suggestions
{"text": "Dark mode"}

# now: the leptoskit RPC operation
POST {endpoint}/api/feedback_sdk_suggestions?project=42
{"project": 42, "input": {"text": "Dark mode"}}
```

Errors keep their `code`, `status`, and `Retry-After`. When a site answers an operation with a `404` that carries no `code`, it predates the RPC operations, so the client switches to the REST routes for the rest of its life, and the worker for the rest of its process. No host code changes are needed. The hosted sign-in page now calls `feedback_iframe_session`, and the host-backend example issues sessions with `feedback_sdk_session` (`{project, input: {user_id, surface}}`); the REST `POST /api/v1/feedback/{project}/sessions` keeps working.

`FeedbackClient` (npm) gains a public `origin` field beside `base`; its method signatures are unchanged. The API classifier treats any change to an exported class as modified, so `@monochange/feedback` takes a major bump.

## monochange_feedback [0.2.0](https://github.com/monochange/monochange/releases/tag/monochange_feedback/v0.2.0) (2026-11-08)

### 💥 Breaking Change

#### Add native feedback conversations to Flutter apps

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #860](https://github.com/monochange/monochange/pull/860)

The first release provides a Dart `FeedbackClient` and a native Flutter `FeedbackSheet`. It supports similar-request suggestions, explicit upvotes, conversations, submission, discussion, and delivery status using the same API as the web and React Native SDKs.

```dart
FeedbackSheet(client: feedbackClient, identityKey: user.id)
```

Provide `getClientToken` through your authenticated backend and change `identityKey` when the user changes. The client refreshes expired sessions, preserves submission keys across interrupted responses, and prevents pending replies from reaching a new account. The example includes Android and iOS hosts.

### 🐛 Fixed

- **Publish the Flutter SDK with the Flutter toolchain.** The `monochange_feedback` pubspec now declares `flutter: uses-material-design: true`, which the sheet needs because it renders Material icons. Declaring the `flutter` section also tells monochange the package is a Flutter package, so release publishing runs `flutter pub publish` instead of `dart pub publish` — the command pub.dev requires for packages that depend on the Flutter SDK. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #875](https://github.com/monochange/monochange/pull/875)
