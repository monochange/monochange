---
"monochange": patch
---

# Speed up release command step evaluation

Avoid building full template contexts for simple release command steps, literal commands, and direct input forwarding. This makes dry-run releases in large workspaces surface progress immediately and evaluate skipped command steps much faster.
