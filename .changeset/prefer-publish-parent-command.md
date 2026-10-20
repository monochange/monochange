---
monochange: fix
monochange_publish: fix
monochange_github: fix
"@monochange/cli": fix
"@monochange/skill": fix
---

# Suggest `monochange publish` subcommands instead of `monochange step` publish steps

Diagnostics, generated files, and help text now point at the `monochange publish` parent command instead of the equivalent `monochange step` publish steps:

- The npm one-time-password recovery hint now reads `monochange publish placeholder --otp <CODE>` instead of `monochange step placeholder-publish --otp <CODE>`.
- Trusted-publishing, readiness, and resume diagnostics suggest `monochange publish packages` and `monochange publish readiness`.
- `monochange init --provider github` writes a release workflow that runs `monochange publish packages`, and the GitHub Actions and GitLab CI batch snippets rendered by `monochange step plan-publish-rate-limits` call `monochange publish packages`.
- `monochange help step publish-packages`, `step publish-readiness`, and `step placeholder-publish` point at the preferred `monochange publish` subcommand.
- The bundled agent skill and the `@monochange/cli` readme recommend the `monochange publish` subcommands, so agents stop suggesting the `monochange step` publish forms.

The `monochange step publish-packages`, `step publish-readiness`, and `step placeholder-publish` commands still work and accept the same inputs, so existing workflows need no change. To adopt the preferred spelling in CI:

```yaml
- run: monochange publish readiness --from HEAD --output .monochange/readiness.json
- run: monochange publish packages --output .monochange/publish-result.json
```
