---
monochange: major
monochange_config: patch
monochange_publish: patch
monochange_schema: patch
monochange_telemetry: patch
---

# Remove colon-delimited built-in step compatibility

> **Breaking change** — colon-delimited top-level built-in step tokens are no longer accepted.
>
> Split each obsolete generated-step token into two arguments: the `step` namespace followed by the step name.

monochange now recognizes built-in steps only through the nested `step <name>` command tree. Obsolete colon-delimited names are no longer parsed, classified, reserved by configuration validation, or suggested by publishing errors, so scripts, telemetry, help text, and configuration all agree on one command shape.

Use the nested invocation:

```nu
monochange step validate
```

Update automation and argument arrays at the same boundary. For example, replace a single generated-step argument with two arguments: `["step", "validate"]`.
