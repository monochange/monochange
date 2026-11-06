# Flutter feedback

The `monochange_feedback` package provides `FeedbackClient`, `ClientToken`, and a native `FeedbackSheet` for Flutter. Users can discuss feedback, discover and upvote similar requests, submit a new request, and follow delivery updates.

Read the [Flutter integration guide](https://github.com/monochange/monochange/blob/main/docs/src/guide/feedback-widget.md#flutter). The `example/` directory contains Android and iOS hosts.

The host supplies `getClientToken`, returning a backend-issued mobile `ClientToken` with an expiry. `ClientToken.fromJson` accepts the session response's Unix-seconds `expires_at`. Optional `getContext` supplies a safe screen route, version, and locale. Never put the backend credential in the Flutter app.

`FeedbackSheet` requires `client` and `identityKey`; optional `onClose`, `onSubmitted`, and `labels` integrate presentation and translations with your app. Change `identityKey` when accounts change. Dispose an owned client with `close()`. API failures expose `FeedbackException.message`, `code`, `status`, and `retryAfter`.
