---
monochange: patch
monochange_core: patch
monochange_publish: patch
monochange_npm: patch
---

# Add npm placeholder publish OTP support

Allow npm placeholder publishing to receive a one-time password for accounts that require 2FA during publish operations.

Before, `mc placeholder-publish` and `mc step:placeholder-publish` could only invoke `npm publish` without an OTP, causing npm `EOTP` failures for publish-time 2FA accounts.

After, pass a fresh code with `--otp`:

```sh
mc placeholder-publish --otp 123456
mc step:placeholder-publish --from HEAD --otp 123456
```

The generated npm process receives the code through `NPM_CONFIG_OTP`, keeping it out of command arguments, reports, and failure messages.
