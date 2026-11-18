# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

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
