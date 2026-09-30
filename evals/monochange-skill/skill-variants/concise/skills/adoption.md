# Adoption checklist

Use this when adding monochange to an existing monorepo.

## Discovery

1. List package ecosystems: Cargo, npm, Deno, Dart/Flutter, Python, Go.
2. Identify package ids that should be release-managed.
3. Identify private packages or applications that should be excluded from releases.
4. Identify groups that must share a version.
5. Identify lockfiles and generated schemas that must refresh after version changes.
6. Identify current release/publish CI jobs and whether monochange should replace or feed them.

## Initial commands

```bash
monochange init
monochange step validate
monochange discover --format json
monochange check
```

Edit the generated config rather than accepting it blindly.

`init` creates an annotated starter config with discovered package ids and a shared group when multiple packages are found. Review that group before adopting it: independent packages need independent release identities. Quote ids containing punctuation in TOML, for example `[package."@acme/api"]`. Dart workspace roots can also be discovered as packages; inspect the actual graph rather than counting directories.

`discover` reports raw inventory across all ecosystems. To restrict managed ownership, edit the generated explicit package tables and groups, or configure `[ecosystems.<name>.auto_discover]` registration. `enabled`, `roots`, and `exclude` alone do not filter inventory or ownership, and auto-discovery exclusions do not remove explicit package tables. Check the resolved registrations with `monochange config --format json`; see [configuration.md](configuration.md#ecosystem-settings).

The starter has no `[cli.*]` workflows. Use built-ins such as `monochange create`, `monochange preview`, and `monochange prepare` immediately; `monochange run release` only exists after a `[cli.release]` definition has been added. `init --provider github` also writes `.github/workflows/release.yml` and `changeset-policy.yml`, replacing files at those paths if they already exist. GitLab and Gitea configure `[source]` without writing those workflows. Review generated source ownership, branch policy, and existing workflow customizations.

`init` refuses an existing config unless `--force` is used. That flag replaces the config; it is not an incremental update. For an existing adoption, edit the config rather than regenerating it.

`init` also refuses multiple package owners discovered in the same directory, such as `Cargo.toml` and `package.json` beside each other. This protects ownership from an ambiguous generated config. Move the packages to separate directories, or create the config manually with one release owner for that path. Different directories with colliding manifest names can use distinct configured ids without renaming manifests. `--force` does not resolve same-path ownership conflicts.

## Updating an existing adoption

There is no `monochange update` command. Choose the operation that matches the requested update:

- **Add customizable workflows:** use `monochange command` for interactive workflow editing or edit TOML directly for automation. `monochange populate` requires an existing config and checks the legacy default-workflow set. That set is currently empty, so it adds nothing and leaves existing workflows unchanged. It does not rediscover packages or upgrade workflow settings.
- **Refresh agent guidance:** after upgrading the CLI through the repository's package manager, inspect `monochange skill` and `monochange skill read monochange`. The bundled skill belongs to that binary version. Refresh an installed copy with `monochange skill install --dir <skill-directory> --force`; review local customizations before replacement. Without `--force`, an existing `SKILL.md` is protected. Read topics directly when no installed copy is needed.
- **Update dependency constraints:** `monochange versions sync --dry-run --format json` previews synchronization against current package versions. Apply with `monochange versions sync` when requested. This does not create release intent or compute a new release.
- **Update release files:** author changesets, run `monochange preview --format json`, then use `monochange prepare` when local version and changelog writes are requested. Preparation consumes applied changesets.

After config edits, run `monochange step validate`, `monochange check`, and `monochange preview --format json`. Validation and linting alone do not reject changesets mixing changelog streams; preview does. In a Python adoption, configure typed `versioned_files` with `fields = ["version"]` for each package's own version. For Go, use tag-based versions and an `initial_version`. See [configuration.md](configuration.md#python-and-go-version-writing).

## Migration questions

- Are packages independently versioned or grouped?
- Which packages get tags or provider releases?
- Which packages publish to public registries?
- Which packages are built-in publishable vs external publishable?
- Which changelog format should be used?
- Which package paths should require changesets in pull requests?
- Which user-defined workflow names should this repository expose?

## Minimal outcome

A good initial adoption has:

- Explicit `[package.*]` entries for managed packages.
- Optional `[group.*]` entries for synchronized versions.
- `[ecosystems.*]` settings for enabled ecosystems and lockfile/versioned-file behavior.
- `[changesets.affected]` if CI checks changeset coverage.
- `[lints]` if `monochange check` should enforce manifest rules.
- Optional `[cli.*]` workflows for team commands that need customization or multiple steps.

## Installation

- **npm**: `npm install -g @monochange/cli`
- **Cargo**: `cargo install monochange`
- **Nix / devenv**: Available via [ifiokjr/nixpkgs](https://github.com/ifiokjr/nixpkgs) flake. Add `inputs.ifiokjr-nixpkgs` to your flake.nix and reference `inputs.ifiokjr-nixpkgs.packages.${pkgs.stdenv.system}.monochange` in your devenv packages. Or run directly: `nix run github:ifiokjr/nixpkgs#monochange`.
