---
"monochange": major
---

# Remove the `mc` binary alias

The release now ships and documents only the `monochange` executable. The packaged `mc` binary alias has been removed.

Before:

```sh
mc check
mc versions --format json
mc step:validate
mc release --dry-run
```

After:

```sh
monochange check
monochange versions --format json
monochange step validate
monochange release --dry-run
```

## Rationale

The CLI should have a single canonical executable name. Keeping a bundled alias made installation archives larger, complicated release archive expectations, and caused documentation and automation to drift between `mc` and `monochange`. Users who prefer a short command can still define their own shell alias or wrapper locally, but monochange no longer installs or maintains that alias as part of the public API.

## Migration guidance

Replace calls to `mc` with `monochange` in CI workflows, local scripts, package manager hooks, agent instructions, and documentation.

If a repository wants to keep a local shorthand, define it outside monochange. For example:

```nu
alias mc = monochange
```

Do not rely on package archives, `cargo binstall`, npm packages, or release downloads containing an `mc` executable. Automation should invoke `monochange` directly so it works consistently across all installation methods.
