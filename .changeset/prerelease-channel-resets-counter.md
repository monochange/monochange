---
"monochange": patch
---

# Restart the prerelease counter when the channel changes

With `numbering = "increment"`, the counter was read from the previous prerelease suffix without comparing the channel, so switching `[prerelease].channel` continued the old sequence. Changing from `alpha` to `beta` produced `1.1.0-beta.5` after a series of alpha prereleases, which misrepresented how many beta builds existed.

A channel switch now starts a fresh sequence at `.0`:

```toml
[prerelease]
enabled = true
channel = "beta" # was "alpha" with latest 1.1.0-alpha.4
numbering = "increment"
```

```text
# before
1.1.0-beta.5

# after
1.1.0-beta.0
```

The channel comparison is case-insensitive because the identifier is lowercased before use, so `channel = "ALPHA"` after `alpha.4` still continues to `alpha.5`. A change of stable base continues to restart the sequence as before.
