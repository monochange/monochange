---
monochange: patch
---

# Keep publish progress lines on their own lines next to the step spinner

Publish progress events (for example `◆ Publishing 56 packages …` and per-package `⏭️`/`✅`/`❌` lines) previously appended to the active CLI step spinner line, so leading symbols appeared mid-line instead of at the start of a new line.

Both reporters now share the global stderr lock, and publish progress clears the active spinner line before writing, so every publish progress line starts on its own line while the step spinner continues animating below it.
