# `monochange_cocoapods`

<!-- {=projectBrandLogo:"https://raw.githubusercontent.com/monochange/monochange/main/assets"} -->

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-dark-280.png">
  <img src="https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-280.png" alt="monochange" width="280" height="171">
</picture>

<!-- {/projectBrandLogo} -->

<br />

<!-- {=crateReadmeBadgeRow:"monochange_cocoapods"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**cocoapods-orange?logo=rust)](https://crates.io/crates/monochange_cocoapods) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**cocoapods-1f425f?logo=docs.rs)](https://docs.rs/monochange_cocoapods/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_cocoapods)](https://codecov.io/gh/monochange/monochange?flag=monochange_cocoapods) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCocoaPodsCrateDocs} -->

`monochange_cocoapods` discovers `CocoaPods` pods from `.podspec` manifests for the shared planner and publishes them to the `CocoaPods` trunk.

Reach for this crate when you need to scan podspecs, keep pod versions in lockstep with the rest of a monorepo, and run `pod trunk push` as part of a planned release.

## Why use it?

- discover pods through Ruby and JSON podspecs
- read pod names and versions from `s.name` and `s.version` declarations
- publish releases to the `CocoaPods` trunk with `pod trunk push`
- validate a push ahead of time with `pod spec lint` during dry runs

## Best for

- scanning Apple platform pods into normalized workspace records
- versioning pods together with the Swift packages and native libraries they ship
- reserving pod names with a placeholder podspec publish

## Public entry points

- `discover_pods(root)` discovers podspecs under a repository root
- `parse_podspec(path, root)` parses one podspec into package metadata
- `update_podspec_version(contents, version)` rewrites the `s.version` declaration
- `write_cocoapods_placeholder_manifest(dir, request)` writes a minimal placeholder podspec

## Scope

- `.podspec` and `.podspec.json` name and version parsing
- trunk publishing through `pod trunk push <podspec> --allow-warnings`
- trunk API metadata for duplicate-version detection
- no trusted publishing: the trunk authenticates through per-machine session tokens

<!-- {/monochangeCocoaPodsCrateDocs} -->
