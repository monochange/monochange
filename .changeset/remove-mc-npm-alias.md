---
"@monochange/cli": major
monochange: patch
monochange_schema: patch
---

# Remove the `mc` npm bin alias

> **Breaking change** — the `@monochange/cli` package no longer installs an `mc` executable.
>
> Invoke `monochange` directly, or add your own shell alias if you want the short name locally.

The Rust binary already shipped only `monochange`; this removes the leftover npm `mc` bin entry so the published package and the migration guide agree.

```nu
# before
mc check
mc versions --format json

# after
monochange check
monochange versions --format json
```

Add a local alias if you still want the short name:

```nu
alias mc = monochange
```

Update any scripts, CI workflows, or docs that call `mc` to use `monochange` instead.
