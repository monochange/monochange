# `monochange_swift`

<!-- {=projectBrandLogo:"https://raw.githubusercontent.com/monochange/monochange/main/assets"} -->

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-dark-280.png">
  <img src="https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-280.png" alt="monochange" width="280" height="171">
</picture>

<!-- {/projectBrandLogo} -->

<br />

<!-- {=crateReadmeBadgeRow:"monochange_swift"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**swift-orange?logo=rust)](https://crates.io/crates/monochange_swift) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**swift-1f425f?logo=docs.rs)](https://docs.rs/monochange_swift/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_swift)](https://codecov.io/gh/monochange/monochange?flag=monochange_swift) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeSwiftCrateDocs} -->

`monochange_swift` discovers Swift Package Manager packages from `Package.swift` manifests for the shared planner.

Reach for this crate when you need to scan Swift packages, resolve their versions from repository tags, and keep them releasing in lockstep with the rest of a mixed-language monorepo.

## Why use it?

- discover Swift packages through `Package.swift` manifests
- model source-control Swift Package Manager releases as repository tags
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
