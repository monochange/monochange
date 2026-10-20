---
monochange: fix
---

# Publish pnpm workspaces with pnpm under trusted publishing

npm-registry packages in a pnpm workspace now always publish with `pnpm publish`, including when `publish.trusted_publishing` is enabled. Before this change, trusted publishing switched these packages to `npm publish`. npm does not understand the `workspace:` protocol, so any internal dependency declared as `workspace:*` or `workspace:^` was published verbatim and broke installs for consumers. pnpm rewrites those ranges to real versions when it packs.

pnpm 11 performs npm's OIDC trusted-publishing exchange natively, so trusted publishing from a pnpm workspace now requires pnpm 11.1.3 or later on the publishing runner:

```yaml
- uses: pnpm/action-setup@v6
  with:
    version: 11
```

pnpm publish and placeholder commands also pass `--no-git-checks`. monochange validates the release commit itself, and release jobs usually run from a detached tag checkout with build outputs in the tree, which pnpm's branch and clean-tree checks would reject. npm, Yarn, and Bun workspaces keep publishing with the `npm` CLI.
