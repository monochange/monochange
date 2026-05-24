---
"@monochange/cli": patch
---

# Refactor npm scripts to TypeScript

Move repository npm tooling and the npm CLI launcher source to TypeScript so local and CI scripts run through Node's native TypeScript support while the published CLI package still ships a built JavaScript bin.
