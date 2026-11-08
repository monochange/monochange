---
monochange_feedback: fix
---

# Publish the Flutter SDK with the Flutter toolchain

The `monochange_feedback` pubspec now declares `flutter: uses-material-design: true`, which the sheet needs because it renders Material icons. Declaring the `flutter` section also tells monochange the package is a Flutter package, so release publishing runs `flutter pub publish` instead of `dart pub publish` — the command pub.dev requires for packages that depend on the Flutter SDK.
