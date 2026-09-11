---
monochange: patch
---

# Stop repeating the step name on every captured output line

Command output no longer carries a `[stdout]` or `[stderr]` tag and no longer repeats the step name as a per-line prefix. The step is named once as a block header, and every captured line is indented beneath it, so a chatty command reads as prose instead of a wall of labels.

Before:

```text
│ format release files [stderr] • Validating lock
│ format release files [stderr] • Validating lock in 2.35ms
│ format release files [stdout] done
```

After:

```text
│ format release files
│   • Validating lock
│   • Validating lock in 2.35ms
│   done
```

The block header is re-established after an interrupting progress line, such as the heartbeat that reports a still-running command. stdout and stderr interleave in arrival order in the human view; `--progress-format json` still reports each `command_output` event with its `stream` and `text` fields, so machine consumers lose nothing.

Publish progress now uses the same symbol set, colors, and ASCII fallback as workflow progress. `--progress-format ascii` no longer emits emoji, outcomes such as `published` and `failed` are colored consistently with workflow steps, and the animated publish line no longer renders two spinner frames.

The Publish complete summary lists only the outcomes that occurred, for example `✖ Publish complete: 2 published, 1 failed`. A run that published nothing uses a neutral marker instead of a success symbol.
