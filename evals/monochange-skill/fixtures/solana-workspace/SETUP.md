# Setup: Pina-shaped Solana workspace

This fixture models the release surface of a Pina monorepo without requiring
the Solana toolchain:

- `programs/transfer` is the on-chain program: a `no_std` Pina crate that
  `pina build` compiles to an SBF `.so` (stub at `target/deploy/transfer.so`;
  the program keypair `target/deploy/transfer-keypair.json` also lives under
  `target/` and is never committed). Solana programs have no registry publish
  and no on-chain version field, so the package sets
  `publish = { enabled = false }` and rides the group tag for identity.
- `programs/transfer/migrations/manifest.json` is Pina's own on-chain schema
  versioning. It is committed source, managed exclusively by `pina migrations`
  commands, and orthogonal to release versioning.
- `apps/sdk` is the npm TypeScript client. It releases in lockstep with the
  program through group `main` (one version, one `vX.Y.Z` tag).
- `idl/transfer.json` is the checked-in Codama IDL refreshed by `pina build`;
  the program version it carries lives at `program.version`, because the
  top-level `version` names the Codama standard. `deploy/mainnet.json` is the
  deploy manifest naming the version being shipped.
- The declared value `transfer.artifact_digest` hashes the stub `.so` so every
  release record carries the digest of the exact binary it ships.
- The `onchain` changelog stream routes program-affecting changes to
  `deploy/upgrade-notes/{{ version }}.md`. That output only exists when a
  release owes an on-chain deployment, which is the deploy-eligibility signal.
- `[cli.deploy]` wires deployment as a user-supplied `Command` step;
  `scripts/deploy-program.sh` stands in for
  `pina deploy --cluster <rpc> ... --yes` (which wraps
  `solana program deploy`) and records its execution in `.deploy-ran` so
  contracts can distinguish a dry run from a real one.

The fixture ships without `.git` and without `.changeset/`. Scenarios that
need release history own it via `setupCommands` and seed the `v1.2.0` baseline
tag; the manifests all start at `1.2.0` to match:

```sh
git init -b main -q && git add -A && git commit -S -qm "chore: initialize transfer workspace" && git tag v1.2.0
```
