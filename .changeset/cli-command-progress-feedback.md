---
monochange: patch
---

# Show workflow progress and preserve command failure output

Configured workflows now report command and step starts on stderr by default, including captured editor and CI runs. Quiet mode and `MONOCHANGE_NO_PROGRESS` still suppress progress. Failed workflows now emit exactly one failed terminal event, run `always_run` cleanup steps, identify later steps skipped after the failure, and never report the failed step or command as successful.

Command:

```bash
monochange run release
```

**Before (stderr):**

```text
# no status while a captured command was running
error: command `build-project` failed: exit status 1
```

**After (stderr):**

```text
monochange running `release`
▶ [1/2] build project (Command)
▶ [1/2] build project (Command) — running command `build-project`
✖ [1/2] build project (Command)
  └─ discovery error: command `build-project` failed: exit status: 1
  │ stdout:
  │ compiler diagnostic
✖ `release` failed
```

Configured command and lockfile command failures preserve the exit status and useful stdout as well as stderr, so users no longer need to rerun the child command to discover stdout-only diagnostics. Package-publish progress also respects quiet mode.
