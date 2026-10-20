---
monochange_app: fix
---

# Document how command output is ordered

The progress output reference now explains the order every multi-line command follows: the plan, its details, the log, the detailed results, and a closing summary line. It includes a placeholder publish example and lists the new `publish_plan` JSON progress event.

The `PublishPackages` and `PlaceholderPublish` step references now say that results end with their summary line, and describe how a registry rate limit is reported and resumed.
