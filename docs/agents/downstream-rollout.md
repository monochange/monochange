# Downstream release rollout

How to roll a monochange release out to the repositories that consume it, and which steps a maintainer must perform by hand.

This is the executable form of the request "kick off the release flow, then update every downstream repo". Work through the phases in order. Each phase produces evidence; do not start a phase until the previous one is verified.

## Phase order and dependencies

The chain is strictly ordered, because each phase consumes an artifact the previous one published.

```text
1. monochange release PR merged        -> vX.Y.Z tag + GitHub release + crates.io + npm
2. monochange/actions release PR merged -> actions vA.B.C tag
3. ifiokjr/nixpkgs package.nix bump     -> downstream repos can resolve the new binary
4. downstream repo updates              -> devenv.lock refresh, monochange/actions pins
5. downstream PRs merged
```

Two rules follow from the chain.

- **Nothing downstream can move before the monochange release is published.** Downstream repositories resolve monochange from `ifiokjr/nixpkgs`, whose `packages/monochange/package.nix` fetches release archives by tag. Until `vX.Y.Z` exists as a real (non-draft) GitHub release with uploaded assets, a downstream bump has nothing to download.
- **`monochange/actions` is a separate release stream.** The action variants are versioned independently of the CLI, so an actions release does not wait on the CLI release and vice versa. Downstream repositories pin action SHAs, so they must be updated separately from the CLI version.

## Human-only steps

These steps are reserved for a maintainer. An agent must not perform them, and must stop and hand back instead.

- Merging the monochange release PR (`chore(release): prepare release` on `monochange/release/release`).
- Merging the `monochange/actions` release PR.
- Triggering `publish.yml`, `release.yml`, or `docs-release.yml` manually.
- Publishing any package to any registry, with or without local credentials.
- Creating, deleting, or modifying tags or GitHub releases.
- Adding the `no-changeset-required` label.

When a phase reaches one of these steps, report the release PR number, its check status, and the merge command the maintainer should run, then continue with any independent work.

## Phase 1 — monochange release

The release PR is maintained automatically. On every push to `main`, the `release-pr` job in `.github/workflows/ci.yml` refreshes it, and when pending changesets exist it also dispatches a publish dry run against the release PR branch so the exact release tree is validated before anyone merges.

Inspect status rather than opening a PR:

```bash
gh pr list --repo monochange/monochange --state open \
  --json number,title,headRefName,statusCheckRollup
```

A release PR is ready to hand to the maintainer when every check is `SUCCESS`. On success the merge is a squash merge; the repository permits only squash merges.

After the maintainer merges, the same push triggers `release-post-merge`, which tags the release, creates the draft GitHub release, and dispatches `publish.yml`. Verify the result:

```bash
gh release view "vX.Y.Z" --repo monochange/monochange --json isDraft,assets
```

The release is published only when `isDraft` is `false` and the asset list contains every target archive (`monochange-<target>-vX.Y.Z.tar.gz` / `.zip`). Downstream work is blocked while the release is still a draft, because the archives are not downloadable yet.

### Pre-merge gate: first-time crates need a placeholder

Before the release PR is merged, confirm every cargo package in the release set already has a registry entry. crates.io rejects the first publish of an unknown crate name, and a single rejection aborts the batch, so the release ends up **partially published**: the npm packages and the crates that sorted before the failure are live, the CLI and everything after it are missing, and the GitHub release stays a draft.

Check each package that is new or newly added to the release group:

```bash
for c in monochange_classification monochange_some_new_crate; do
  curl -s -o /dev/null -w "$c: %{http_code}\n" "https://index.crates.io/${c:0:2}/${c:2:2}/$c"
done
```

`404` means the name is unregistered. Run `monochange step placeholder-publish` (it is a manual step — see [release prerequisites](release-checklist.md)) and re-check until every name resolves, **before** merging the release PR.

### Recovering a partial publish

A partial publish is not fixable by re-running the release PR. Recover in this order:

1. Publish the missing placeholder for the unregistered crate, then verify the name resolves.
2. Re-dispatch `publish.yml` for the existing tag (`workflow_dispatch` with `tag: vX.Y.Z`). The publish step checks each package against its registry, so already-published versions are skipped and only the missing ones are attempted.
3. Confirm the GitHub release flips to non-draft and every archive becomes downloadable.

Do not delete or re-point the release tag to work around this: the tag is the release record's identity and downstream repositories resolve assets by it.

## Phase 2 — monochange/actions release

`monochange/actions` has its own release stream and its own guarded merge workflow (`.github/workflows/release-pr-merge.yml`). It exposes two entry points, both maintainer-controlled:

- a `workflow_dispatch` run, which fast-forwards the release PR;
- a comment of `/merge` on the release PR, which does the same from the issue thread.

Verify the actions release the same way:

```bash
gh release view "vA.B.C" --repo monochange/actions --json isDraft
```

Note the version because downstream pins reference it. Action pins are recorded as a full commit SHA with a trailing version comment, for example `monochange/actions/changeset-policy@<sha> # v0.9.3`.

## Phase 3 — ifiokjr/nixpkgs

Downstream repositories resolve the monochange binary through `ifiokjr/nixpkgs`. The package lives in `packages/monochange/package.nix` and pins both a `version` and per-platform `sha256` hashes of the published archives.

The update is automated. `.github/workflows/update.yml` runs `nu scripts/update` daily at 06:00 UTC and on `workflow_dispatch`; the `update-monochange` function in `scripts/update` reads the latest GitHub tag, rewrites `version`, recomputes every platform hash, and the workflow commits the result as `chore: update packages <date>`.

To run it by hand:

```bash
cd ../nixpkgs
nix shell nixpkgs#nushell -c nu scripts/update
```

A hand update changes the same fields:

1. set `version` in `packages/monochange/package.nix`;
2. replace each `sha256-…` in the `hashes` attrset with the hash of the new archive (a wrong hash fails the build, so never guess — fetch and hash the real artifact);
3. keep `README.md`'s version table in sync (`scripts/versions` feeds the table through `mdt`).

Verify before opening a PR:

```bash
nix build .#monochange
```

A stale `packages/monochange/package.nix` is the single most common cause of a downstream bump failing, because the devenv lock refresh resolves to an `ifiokjr/nixpkgs` revision that still carries the old version.

## Phase 4 — downstream repositories

Inventory the repositories that consume monochange before changing anything. A repository is a consumer if it has a `monochange.toml`, references `monochange/actions` in `.github`, or pulls `custom.monochange` / `extra.monochange` from `inputs.ifiokjr-nixpkgs`.

Two independent updates apply, and a repository may need either or both.

### 4a. Binary version via the devenv lock

Repositories that take monochange from `ifiokjr-nixpkgs` record the resolved revision in `devenv.lock`. Bumping the lock picks up the new binary and is what the `chore: update monochange to X.Y.Z` PRs contain — the diff is usually only `devenv.lock`.

```bash
cd <repo>
devenv update
nix flake update   # when the repo uses flake.nix instead of devenv.yaml
```

Confirm the resolved version before opening a PR:

```bash
monochange --version
```

### 4b. Action SHA pins

Repositories that call monochange action variants pin a full commit SHA. Every monochange release that changes an action — especially a breaking one — requires re-pinning, because the pin cannot float.

Locate the pins:

```bash
grep -rn "monochange/actions" .github/
```

Each reference looks like `owner/action@<40-char-sha> # vX.Y.Z`; replace the SHA with the released tag's commit and update the comment. Keep the existing format, and pin the exact release rather than a branch.

When a release is breaking, check the pinned action's inputs before assuming the bump is safe: an input added in a newer release is absent from an older pin, so moving a pin **backwards** silently drops capability. `monochange/actions/change-classification` is the common case — the `labels` input does not exist before `v0.9.4`, so a repository that relies on label-driven skipping must not be pinned below it.

### Downstream PR shape

- Branch name: `chore/monochange-X.Y.Z` for a binary bump, or the existing action-pin convention.
- One concern per PR: do not mix the binary bump and the action-pin update unless the repository's own conventions already combine them.
- Let CI finish. Downstream repositories carry their own gates (`check`, `lint`, `test`, `zizmor`), and a monochange bump that breaks a repository's `monochange.toml` surfaces there.
- Repositories with an open release PR of their own must not have that PR merged to satisfy this rollout; it belongs to their own release cycle.

## Phase 5 — verify the rollout

For each repository touched, confirm the merged default branch resolves the intended version. Re-run the inventory in phase 4 and check that no consumer still pins the previous release, then report the table of repository, previous version, new version, and PR link.

## Failure modes worth naming

- **A new crate was never placeholder-published.** crates.io rejects the first publish of an unknown name, the batch aborts, and the release lands partially published with the GitHub release still a draft. This is the highest-cost failure in the chain because it can only be cleared by publishing a placeholder and re-dispatching `publish.yml`.
- **Downstream bump builds the old binary.** The `ifiokjr/nixpkgs` bump (phase 3) has not landed, or the repository's lock refresh resolved a revision from before it.
- **Downgraded action capability.** An action pin was moved to a tag that predates an input the workflow passes; inputs are ignored silently, so the workflow passes while doing less.
- **Release still a draft.** Phase 4 attempted while the GitHub release was a draft, so archives were not downloadable.
- **Stale release PR misread as failure.** A release PR branch that is behind `main` reports failures from `main`'s newer commits, not from its own content. Refresh the branch rather than debugging the reported failure.
