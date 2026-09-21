---
"monochange": patch
---

# Report no planned versions instead of failing when no changesets are pending

`DisplayVersions` required at least one changeset, so asking for the next version between releases failed with a configuration error:

```bash
monochange next
# before: error[config.invalid]: no markdown changesets found under .changeset  (exit 1)
# after:  no package or group versions were planned                             (exit 0)
```

An empty `.changeset` directory is a normal state, and the command now reports it in the selected format rather than treating it as an error. `text` prints `no package or group versions were planned`, `markdown` prints `No package or group versions were planned.`, and `json` emits empty maps:

```json
{
	"packages": {},
	"groups": {}
}
```

`PrepareRelease` is unchanged: it still requires changesets unless a command sets `allow_empty_changesets = true`.
