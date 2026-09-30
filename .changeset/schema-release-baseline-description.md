---
monochange_schema: patch
---

# Clarify the release-baseline schema description

The embedded configuration schema now describes tag-based version resolution as selecting the highest matching repository tag. The previous description incorrectly implied that lookup only considered tags reachable from the current commit. Configuration fields and accepted values remain unchanged.
