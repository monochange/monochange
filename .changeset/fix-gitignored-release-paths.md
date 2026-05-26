---
monochange: patch
monochange_github: patch
---

# Skip gitignored release staging paths

Release staging now skips gitignored paths before git inspection, avoiding failures on ignored symlink descendants such as FVM Flutter SDK files.
