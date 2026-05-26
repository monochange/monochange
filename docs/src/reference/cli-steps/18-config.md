# `Config`

## What it does

`Config` renders the resolved monochange configuration and workspace metadata.

Use it when you need to inspect the configuration after defaults, package discovery, source settings, lint settings, and workflow definitions have been loaded into monochange's execution model.

## Why use it

`Config` is a read-only inspection step. It is useful for:

- debugging why a workflow input, package selector, or source setting resolved the way it did
- capturing configuration state in CI artifacts
- checking generated or hand-written `monochange.toml` before running release workflows

## Inputs

The direct command is exposed as:

```bash
mc step:config
```

It does not require step-specific inputs.

## Prerequisites

A readable monochange workspace configuration.

## Side effects and outputs

`Config` does not mutate files, create release state, or contact package registries. It renders the resolved configuration and workspace metadata for review.

## Example

```bash
mc step:config
```

In a workflow, use it before mutating steps when you want a durable diagnostic snapshot:

```toml
[cli.inspect]
help_text = "Render resolved monochange configuration"

[[cli.inspect.steps]]
type = "Config"
```
