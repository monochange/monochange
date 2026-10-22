# `monochange_gradle`

<!-- {=monochangeGradleCrateDocs} -->

`monochange_gradle` discovers Gradle projects from `gradle.properties` manifests for the shared planner and publishes their libraries to Maven Central.

Reach for this crate when you need to scan Kotlin and Java libraries, keep `VERSION_NAME` in lockstep with the rest of a monorepo, and run `./gradlew publishAndReleaseToMavenCentral` as part of a planned release.

## Why use it?

- discover publishable Gradle projects through the `VERSION_NAME` convention
- read Maven coordinates from `GROUP`/`POM_GROUP_ID` and `POM_ARTIFACT_ID` properties
- publish releases to Maven Central through the Sonatype Central Portal
- detect already-published versions from Maven Central metadata

## Best for

- scanning Android and JVM libraries into normalized workspace records
- versioning Kotlin bindings together with the native libraries they wrap
- releasing to Maven Central with the `com.vanniktech.maven-publish` task set

## Public entry points

- `discover_gradle_projects(root)` discovers Gradle projects under a repository root
- `parse_gradle_properties(path, root)` parses one `gradle.properties` manifest into package metadata
- `parse_properties(contents)` parses Java properties text into an ordered map
- `maven_central_metadata_path(group_id, artifact_id)` renders the repository path for a coordinate

## Scope

- `gradle.properties` `VERSION_NAME` parsing with properties-file quoting rules
- Maven coordinate resolution from Android publishing conventions
- `rootProject.name` fallback names from `settings.gradle` and `settings.gradle.kts`
- no trusted publishing: Maven Central authenticates through portal user tokens and GPG keys

<!-- {/monochangeGradleCrateDocs} -->
