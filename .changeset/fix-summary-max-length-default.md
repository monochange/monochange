---
"monochange_config": patch
"monochange_core": patch
---

# Fix summary max_length default for headings

The changesets/summary lint rule now only applies the default 60-character length limit to markdown headings. Non-heading summary text (plain first lines) is no longer limited by default.

A new `max_heading_length` option (default: 60) controls the heading-specific length limit independently. Set `max_length` explicitly to enforce a length limit on any summary text, regardless of heading format.
