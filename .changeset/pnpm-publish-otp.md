---
monochange_publish: patch
---

# Pass npm one-time passwords to pnpm 12 publishes

`monochange publish placeholder --otp <CODE>` and OTP-enabled release publishes now set `PNPM_CONFIG_OTP` alongside `NPM_CONFIG_OTP` on the publish command. pnpm 12 ignores `NPM_CONFIG_*` variables, so pnpm workspaces previously failed with `ERR_PNPM_OTP_NON_INTERACTIVE` even when a code was supplied. The OTP recovery hints now name both variables.
