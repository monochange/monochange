---
monochange_app: fix
---

# Bound OAuth work and reject callback replays

`/login` and `/auth/callback` now consume the anonymous address bucket before server rendering. Valid signed callback state is atomically consumed in SQLite before the provider exchange, so replaying the original cookie cannot trigger more exchanges, including after a process restart. Failed exchanges also consume their state; start a fresh sign-in attempt to retry.

The application runs migration `006_create_oauth_state_uses` on startup. Consumed states are retained until their signed expiry and pruned during subsequent callbacks. Provider token requests have a ten-second timeout, and each app process permits at most sixteen concurrent exchanges. Saturated requests can retry their existing state because capacity rejection happens before consumption. Existing rate-limit environment variables continue to control the address bucket.
