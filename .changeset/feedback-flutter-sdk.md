---
monochange_feedback: major
---

# Add native feedback conversations to Flutter apps

The first release provides a Dart `FeedbackClient` and a native Flutter `FeedbackSheet`. It supports similar-request suggestions, explicit upvotes, conversations, submission, discussion, and delivery status using the same API as the web and React Native SDKs.

```dart
FeedbackSheet(client: feedbackClient, identityKey: user.id)
```

Provide `getClientToken` through your authenticated backend and change `identityKey` when the user changes. The client refreshes expired sessions, preserves submission keys across interrupted responses, and prevents pending replies from reaching a new account. The example includes Android and iOS hosts.
