# Install feedback in a website or app

The web, React Native, and Flutter SDKs connect your app's users to a monochange project's feedback. Users describe a bug or change, see similar requests to upvote, submit a new request, discuss details, and follow delivery status. Their existing app account supplies their identity; they do not need a GitHub account.

## Register the app

Open the project's feedback console and choose **Register an app**. Enter a name, an account namespace, and its surfaces. Web integrations need exact HTTPS origins, such as `https://app.example.com`; HTTP is accepted only for localhost development. Wildcards and origins containing a path are rejected.

Share an account namespace between web and mobile only when they use the same account system. Within a project, the same host user id in that namespace has one vote per request. Unrelated account systems need different namespaces. Existing GitHub portal accounts are separate.

Copy the returned backend credential immediately and keep it in your backend's secret store. It is displayed once. Rotation invalidates earlier sessions; revoking an app prevents all its credentials and sessions from writing again.

## Issue client tokens through your backend

After authenticating the host user, call:

```http
POST /api/v1/feedback/123/sessions
Authorization: Bearer <backend credential>
Content-Type: application/json

{"user_id":"your-authenticated-user-id","surface":"web"}
```

Use `surface: "mobile"` for React Native and Flutter. The response is `{ "token": "…", "expires_at": 1793740000 }`, with expiry in Unix seconds. Sessions last ten minutes. Your host token endpoint should return this response only to its authenticated user. Derive `user_id` from the server-side session, never from a user-supplied request field. Do not put the backend credential in a browser bundle, native app, URL, or log.

The feedback API uses bearer tokens and no third-party cookies. Web requests must include their registered browser origin. Mobile sessions reject browser-origin headers. Clients refresh an expired token through the host callback and never receive maintainer or worker permissions.

## Website

```ts
import { mountFeedback } from "@monochange/feedback";

const feedback = mountFeedback({
	endpoint: "https://monochange.dev",
	project: 123,
	getClientToken: async () => {
		const response = await fetch("/api/feedback-session", {
			credentials: "same-origin",
		});
		if (!response.ok) throw new Error("Sign in to share feedback");
		return response.json();
	},
	getContext: () => ({ route: location.pathname, app_version: "1.0.0" }),
	theme: "auto",
});

// Open from any existing app control.
feedback.open();
// Erase draft state and credentials on logout or account change.
feedback.reset();
// Remove the widget and listeners when unmounting the host view.
feedback.destroy();
```

Use `launcher: false` for a custom launcher, or supply an existing element. `target` controls where the widget mounts. The Shadow DOM isolates styles. `locale` sets the dialog language, and `labels` overrides visible strings. Content is rendered as text, never as user-supplied HTML. `open(itemId)` opens a saved submission's status and discussion.

For script-tag installation, load `/embed/feedback.js`, then call `monochangeFeedback.mountFeedback` with the same options. Pin the script to your deployment when you need a fixed client version; versioned package artifacts also contain `dist/embed/feedback.iife.js`. A site's Content Security Policy must allow the script source and the feedback API in `connect-src`. Pass the host document's stylesheet nonce as `styleNonce` when its `style-src` requires a nonce. The widget creates DOM nodes without HTML string injection, so it also works with Trusted Types enforcement.

Use `FeedbackClient` from `@monochange/feedback/client` for headless integrations. It exports `suggest`, `startConversation`, `sendMessage`, `submit`, `getItem`, `reply`, `vote`, `reset`, and `destroy`.

The repository includes a [React host wrapper](https://github.com/monochange/monochange/blob/main/packages/monochange__feedback/examples/react.tsx). It mounts the widget once per stable options object, clears drafts when the host account changes, and disposes listeners when the component unmounts.

## React Native

```tsx
import {
	FeedbackClient,
	FeedbackSheet,
} from "@monochange/feedback-react-native";

const client = new FeedbackClient({
	endpoint: "https://monochange.dev",
	project: 123,
	getClientToken: () => authenticatedHostApi.feedbackSession("mobile"),
	getContext: () => ({ route: "Invoices", app_version: "1.0.0" }),
});

<FeedbackSheet
	client={client}
	visible={open}
	identityKey={user.id}
	onClose={() => setOpen(false)}
/>;
```

Keep the client stable across component renders. Change `identityKey` on logout or account switches. The sheet uses native React Native controls, honors light and dark themes, and refreshes status after resuming. Override `labels` for host translations. Call `client.destroy()` when its owner unmounts permanently.

## Flutter

```dart
import 'package:monochange_feedback/monochange_feedback.dart';

final client = FeedbackClient(
  endpoint: Uri.parse('https://monochange.dev'),
  project: 123,
  getClientToken: () => authenticatedHostApi.feedbackSession('mobile'),
  getContext: () => {'route': 'Invoices', 'app_version': '1.0.0'},
);

FeedbackSheet(client: client, identityKey: user.id);
```

Embed the sheet in a page or use `showModalBottomSheet` with `isScrollControlled: true`. It uses native Flutter controls and the host theme. `labels` supplies translations. Change `identityKey` on account changes and call `client.close()` when its owner is disposed. The package's example includes Android and iOS hosts; run it with `FEEDBACK_PROJECT`, `FEEDBACK_ENDPOINT`, and your authenticated `FEEDBACK_TOKEN_URL` as Dart defines.

## Conversations and submissions

All SDKs use `/api/v1/feedback/{project}`:

| Method       | Path                           | Purpose                                                               |
| ------------ | ------------------------------ | --------------------------------------------------------------------- |
| POST         | `/suggestions`                 | Search visible requests without creating an item                      |
| POST         | `/conversations`               | Start a private draft with `kind`, `message`, and optional `context`  |
| GET          | `/conversations/{id}`          | Recover the current user's private draft                              |
| POST         | `/conversations/{id}/messages` | Add a message with `body` and the expected draft `version`            |
| POST         | `/items`                       | Submit `{conversation, version}` with an `Idempotency-Key` header     |
| GET          | `/items/{id}`                  | Read an owned or visible request, its discussion, and delivery status |
| POST         | `/items/{id}/messages`         | Add a discussion reply with `body`                                    |
| PUT / DELETE | `/items/{id}/vote`             | Add or remove the current user's vote                                 |

Choosing a match does not silently submit or vote. The SDKs preserve submission keys across retries. Stale draft versions return `409`; reload the draft before editing further. Errors have `code` and `message`; quota failures return `429` with `Retry-After`. Quotas are persisted per project, app, and subject, while the existing server middleware also limits client addresses. Refreshing a token cannot reset a user's quota.

Draft conversations are private and expire after 30 days. Sessions expire after ten minutes; idempotency records are kept for a day. The current storage uses the project's versioned feedback document, so very large projects should plan a move to per-item storage before exhausting document limits.

## Context and privacy

The first release accepts text and explicit route or screen context, app version, locale, and an optional stable element identifier. SDKs strip query strings and fragments from supplied routes. Prefer safe route templates over URLs containing account data. Screenshots, recordings, DOM scraping, and automatic logs are separate future opt-ins.

Suggestions contain only requests permitted on the project's user-facing roadmap. Plans, private drafts, held reports, and maintainer discussion never become suggestion candidates. Existing disclosure and screening rules apply to published titles, discussion, repository briefs, and model input.

For semantic suggestions, deployment operators configure `OPENROUTER_API_KEY` through Monosecret and set `MONOCHANGE_FEEDBACK_MODEL`. `MONOCHANGE_FEEDBACK_COMPLETIONS_URL` defaults to OpenRouter's chat-completions endpoint and can point to a trusted compatible service. Replies use [structured output](https://openrouter.ai/docs/guides/features/structured-outputs); returned request ids must match the visible candidate set. Provider failures preserve the draft and fall back to keyword matching. No provider key reaches an SDK.

See [Connected feedback agents](feedback-connected-agents.md) to turn requests into reviewed implementation plans.
