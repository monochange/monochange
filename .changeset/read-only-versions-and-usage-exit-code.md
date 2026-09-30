---
"monochange": change
---

# Make bare `monochange versions` read-only and exit with status 2 for usage errors

Bare `monochange versions` used to rewrite internal dependency constraints in package manifests, with only a deprecation warning. It is now a read-only check: it reports what `monochange versions sync` would change and never writes files. Run `monochange versions sync` to apply the changes.

```bash
monochange versions        # reports pending constraint updates, writes nothing
monochange versions sync   # writes them
```

Command-line usage errors (`error[cli.usage]`, such as an unknown command or flag, and `error[cli.json_required]`) now exit with status `2`, matching common CLI conventions, so scripts can tell a mistyped invocation from a failed release step, which still exits with `1`. Update scripts that expect `1` for usage errors.
