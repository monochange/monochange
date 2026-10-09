---
monochange: fix
monochange_config: fix
"@monochange/cli": fix
---

# Wire yarn.lock through release preparation

The `monochange` crate reads and rewrites `yarn.lock` versioned files during release preparation, and `monochange_config` accepts `yarn.lock` as a typed npm-family versioned-file path so `monochange check` validates entries that target it. The npm CLI wrapper readme lists Yarn alongside npm, pnpm, and Bun.
