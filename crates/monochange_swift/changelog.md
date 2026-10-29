# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.21.1](https://github.com/monochange/monochange/releases/tag/v0.21.1) (2026-10-29)

### Changed

- **No package-specific changes were recorded; `monochange_swift` was updated to 0.21.1 as part of group `main`.**

## [0.21.0](https://github.com/monochange/monochange/releases/tag/v0.21.0) (2026-10-22)

### 💥 Breaking Change

#### Add swift, cocoapods, and gradle ecosystems

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #830](https://github.com/monochange/monochange/pull/830)

monochange now plans and publishes native Apple and Android packages alongside the existing ecosystems: Swift Package Manager packages (`Package.swift`, versions resolved from repository tags), CocoaPods pods (`.podspec` and `.podspec.json`, published to the CocoaPods trunk with `pod trunk push`), and Gradle libraries (`gradle.properties` with the `VERSION_NAME` convention, published to Maven Central through `./gradlew publishAndReleaseToMavenCentral`).

Configure the new ecosystems with `[ecosystems.swift]`, `[ecosystems.cocoapods]`, and `[ecosystems.gradle]`, or type packages directly:

```toml
[package.apple]
path = "platforms/apple"
type = "swift"

[package.pod]
path = "platforms/pod"
type = "cocoapods"

[package.android]
path = "platforms/android"
type = "gradle"
```

Swift support uses source-control distribution through tags and provider releases. Swift package registries are outside the built-in publish flow. Set `trusted_publishing = false` for cocoapods and gradle packages since neither the trunk nor Maven Central supports OIDC trusted publishing. Duplicate-version detection queries the CocoaPods trunk API and `repo1.maven.org` Maven metadata, and typed versioned files rewrite active podspec `s.version` declarations and selected Gradle properties during release preparation.

Gradle publish dry runs append `--dry-run` so Gradle only displays its task graph. CocoaPods dry runs retain the selected podspec and validation flags when switching to `pod spec lint`. Both CocoaPods and Gradle require real releases and do not support placeholder name reservation.
