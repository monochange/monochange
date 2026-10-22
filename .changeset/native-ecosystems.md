---
monochange: feat
monochange_core: feat
monochange_config: feat
monochange_publish: feat
monochange_swift: major
monochange_cocoapods: major
monochange_gradle: major
---

# Add swift, cocoapods, and gradle ecosystems

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
