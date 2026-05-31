---
"monochange_config": patch
---

Fix summary lint max_length to apply only to headings by default

The `changeset/summary` lint rule now only applies the default 60-character length limit to markdown headings. Non-heading summary text (plain first lines) is no longer limited by default. Users can still set `max_length` explicitly to enforce a limit on any summary text, regardless of heading format.

A new `max_heading_length` option (default: 60) controls the heading-specific length limit independently. Set `max_heading_length` to override the default heading length without affecting non-heading summaries.
