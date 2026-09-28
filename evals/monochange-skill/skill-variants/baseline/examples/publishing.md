# Publishing workflow

Readiness and bootstrap are built-in commands:

```bash
monochange step publish-readiness --from HEAD --output readiness.json
monochange step placeholder-publish
```

A repository workflow wraps publish planning and publishing. Every input a step binds must also be declared on the command, so the example below declares `format`, `package`, and `readiness` before passing them:

```toml
[cli.publish-plan]
help_text = "Plan package publishing"

[[cli.publish-plan.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-plan.inputs]]
name = "package"
type = "string_list"
help_text = "Limit the plan to these package ids"

[[cli.publish-plan.inputs]]
name = "readiness"
type = "path"
help_text = "Readiness artifact from a publish-readiness run"

[[cli.publish-plan.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"
inputs = ["format", "package", "readiness"]

[cli.publish]
help_text = "Publish package artifacts"

[[cli.publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish.inputs]]
name = "package"
type = "string_list"
help_text = "Limit the publish to these package ids"

[[cli.publish.inputs]]
name = "output"
type = "path"
help_text = "Write the publish result JSON so a retry can resume"

[[cli.publish.inputs]]
name = "resume"
type = "path"
help_text = "Skip package versions already published by an earlier run"

[[cli.publish.steps]]
name = "publish packages"
type = "PublishPackages"
inputs = ["format", "package", "resume", "output"]
```

`PlanPublishRateLimits` and `PublishPackages` accept the same `package`, `group`, and `ecosystem` selectors, so add those inputs to the command when a workflow needs to narrow or widen the publish set. Keep the JSON artifacts from every run: `--resume` reads the previous result to skip versions that already published, which is how a partial failure recovers without republishing.
