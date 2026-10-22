# `monochange_swift`

<!-- {=monochangeSwiftCrateDocs} -->

`monochange_swift` discovers Swift Package Manager packages from `Package.swift` manifests for the shared planner.

Reach for this crate when you need to scan Swift packages, resolve their versions from repository tags, and keep them releasing in lockstep with the rest of a mixed-language monorepo.

## Why use it?

- discover Swift packages through `Package.swift` manifests
- model Swift Package Manager releases as repository tags, the only distribution channel SPM resolves
- plan lockstep releases across Swift, Rust, npm, and the other supported ecosystems

## Best for

- scanning Swift packages into normalized workspace records
- versioning Apple platform bindings together with the core libraries they wrap
- releasing Swift packages through tag-based group releases and provider release notes

## Public entry points

- `discover_swift_packages(root)` discovers Swift packages under a repository root
- `parse_package_swift(path, root)` parses one `Package.swift` manifest into package metadata
- `parse_package_name(contents)` extracts the package name from manifest text

## Scope

- `Package.swift` name parsing
- tag-based version resolution with no manifest version field
- no registry publish target: Swift packages are consumed from repository tags

<!-- {/monochangeSwiftCrateDocs} -->
