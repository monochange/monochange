# React Native feedback

Install `@monochange/feedback-react-native` with React 18+ and React Native 0.76+. It exports `FeedbackSheet` and `FeedbackClient`, with native controls for conversations, suggestions, votes, submissions, replies, and delivery status.

Read the [mobile integration guide](https://github.com/monochange/monochange/blob/main/docs/src/guide/feedback-widget.md#react-native).

`FeedbackSheet` requires a stable `client`, `visible`, `identityKey`, and `onClose`. Optional properties are `onSubmitted`, `theme`, and `labels`. Changing `identityKey` clears the prior account's draft and credentials. The component refreshes delivery status while visible and after resuming; the host owns the client's final `destroy()` call.

Use backend-issued **mobile** sessions. No backend credential or model key belongs in the app bundle, and no WebView is required. The example `App.tsx` shows host authentication, a custom launcher, and both mobile platform configurations.
