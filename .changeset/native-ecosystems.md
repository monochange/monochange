---
monochange: feat
monochange_core: feat
monochange_config: feat
monochange_publish: feat
monochange_swift: feat
monochange_cocoapods: feat
monochange_gradle: feat
---

# Add swift, cocoapods, and gradle ecosystems

monochange now plans and publishes native Apple and Android packages alongside the existing ecosystems: Swift Package Manager packages (`Package.swift`, versions resolved from repository tags), CocoaPods pods (`.podspec` and `.podspec.json`, published to the CocoaPods trunk with `pod trunk push`), and Gradle libraries (`gradle.properties` with the `VERSION_NAME` convention, published to Maven Central through `./gradlew publishAndReleaseToMavenCentral`).

Configure the new ecosystems with `[ecosystems.swift]`, `[ecosystems.cocoapods]`, and `[ecosystems.gradle]`, or type packages directly:

```toml
[package.apple]
path = "platforms/apple"
type = "swift"

[package.pod]
path = "platforms/apple"
type = "cocoapods"

[package.android]
path = "platforms/android"
type = "gradle"
```

Swift packages release through the tag and provider-release steps because the Swift Package Manager has no registry; set `trusted_publishing = false` for cocoapods and gradle packages since neither the trunk nor Maven Central supports OIDC trusted publishing. Duplicate-version detection queries the CocoaPods trunk API and `repo1.maven.org` Maven metadata, and typed versioned files now rewrite podspec `s.version` declarations and Gradle `VERSION_NAME` properties during release preparation.
