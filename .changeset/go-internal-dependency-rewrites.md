---
"monochange": fix
"monochange_go": minor
---

# Rewrite internal Go `require` directives during release preparation

A Go monorepo could release an internal module while its dependents kept requiring a version that no longer existed. `monochange prepare` rewrote internal dependency constraints for cargo, npm, deno, and dart workspaces, but a `require github.com/acme/core v1.2.0` line in `service/go.mod` stayed stale after `core` moved to `v1.3.0`.

A Go module is identified by the full path its own `go.mod` declares, so the rewrite now reads each workspace package's `module` directive and resolves a `require` against those paths exactly. Rewrites carry Go's `v` prefix, preserve `replace` directives, quoted module paths, and trailing comments, and handle `/v2`-style major suffixes.

Matching by anything looser is unsafe: resolving a `require` on its last path segment alone rewrites an unrelated third-party module that happens to share it, so `github.com/other/core` would receive the workspace's version while `github.com/acme/core` was the intended target. A `require` that no workspace module path resolves exactly is left untouched rather than guessed at.

`monochange versions sync` had the same matching defect, and on a pure Go workspace it never reported anything because tag-versioned packages carry no manifest version for the version map. The sync plan now seeds canonical versions for tag-versioned packages from release tags — the same baselines release planning resolves — before detecting stale constraints, and reports changes keyed by the full module path (`github.com/acme/core`), matching what `apply` rewrites.
