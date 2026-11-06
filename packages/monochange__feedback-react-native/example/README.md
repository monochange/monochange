# Android and iOS host integration

Import `App.tsx` into an existing React Native Android or iOS app and pass its authenticated `userId`, monochange project id, and `getFeedbackSession` callback. The callback must use the host app's existing authenticated HTTP transport and return a mobile feedback session. The SDK has no native modules or CocoaPods dependencies.

On Android, ensure the host manifest includes `android.permission.INTERNET`. On iOS, use HTTPS for both the host token endpoint and monochange; no App Transport Security exceptions are required. Keep the client's identity key aligned with the host account, including logout. Both platforms use the same native sheet and resume behavior.
