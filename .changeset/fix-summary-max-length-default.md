---
"monochange_config": patch
---

Remove default 60-character max_length on changeset summary headings

The changesets/summary lint rule no longer enforces a 60-character maximum on summary heading text by default. Set `max_length` explicitly in your lint configuration if you want to enforce a heading length limit.
