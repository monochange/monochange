---
monochange_config: minor
---

# Add require_description to summary lint rule

The `changesets/summary` lint rule now supports a `require_description` option that ensures the summary heading is followed by at least one non-empty paragraph (not another heading). When enabled, a changeset with only a heading and no description body will fail validation.

Additionally, `max_length` now defaults to 60 characters when the rule is activated. Users can override this by setting `max_length` explicitly in the rule options.
