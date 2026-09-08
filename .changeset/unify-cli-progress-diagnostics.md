---
monochange: patch
---

# Explain command progress and failures without debug logging

monochange now uses one progress reporter for workspace loading, validation, linting, publishing, workflow steps, and subprocess output. Interactive terminals can animate, while CI and captured output receive the same events as complete lines without terminal control sequences.

Failures now include a stable diagnostic code, relevant command or file context, and a suggested next action when one is available. Failed checks keep their full result on stdout, so automation can inspect it without searching through stderr.

Use `--progress-format json` for a newline-delimited machine event stream. `--log-level debug` remains available for maintainer tracing and now disables animation so trace records stay readable.
