---
"@monochange/skill": patch
---

# Document version-prefix controls for internal dependencies

Agents following the skill had no way to discover that internal dependency prefixes are configurable: the `prefix` entry option and the `dependency_version_prefix` ecosystem setting appeared only as bare keys in examples. The skill's configuration module and quick-start rules now state where prefixes come from and how to override them, so agents stop proposing `monochange versions sync --strategy tilde`-style invocations that do not exist and reach for the supported controls instead.

`monochange versions sync --strategy` writes fixed per-ecosystem prefixes (`^` caret ranges, `>=` compatible ranges, bare exact versions, `v` for Go) and never writes `~` or `=`. Custom prefixes belong on typed `versioned_files` entries:

```toml
versioned_files = [
	# write internal npm dependencies as tilde ranges, e.g. "~1.2.3"
	{ path = "package.json", type = "npm", fields = ["dependencies"], prefix = "~" },
]

[ecosystems.npm]
# fallback prefix for typed versioned files without their own prefix
dependency_version_prefix = "^"
```

The entry `prefix` wins over `[ecosystems.<name>] dependency_version_prefix`, which wins over the ecosystem default; both affect internal dependency references only, while `format` and `regex` entries keep writing bare versions.
