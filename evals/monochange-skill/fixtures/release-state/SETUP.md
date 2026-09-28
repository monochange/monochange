# Setup: building the committed release history

This fixture ships without `.git` and without `.monochange/` release state. Scenarios that
exercise the release lifecycle (`step tag-release`, `step release-record`,
`step publish-readiness`, `step retarget-release`) must first create real git objects and a
committed release record. The harness can do that with `setupCommands`; the sequence below is
the exact, verified command list. It has been run end to end against
`<repo>/target/debug/monochange`, producing the tag chain
`v1.0.0` (import) -> `v1.0.1` (patch) -> `v1.1.0` (minor).

The workspace `monochange` binary must resolve to the build under test (the harness prepends
`<repo>/target/debug` to `PATH`). Inline `-c user.name` / `-c user.email` keep the sequence
working on machines without global git identity.

## 1. Initial import and the v1.0.0 tag

```sh
git init -b main
git add -A
git -c user.name="Acme Maintainer" -c user.email="maintainer@acme.dev" \
  commit -m "chore: initial import of the Acme platform services"
git tag v1.0.0
```

## 2. First release: patch to v1.0.1

```sh
mkdir -p .changeset
cat > .changeset/fix-retry-backoff.md <<'EOF'
---
"@acme/api": patch
---

Retry now doubles its backoff between failed attempts instead of retrying immediately.
EOF

monochange prepare
monochange step commit-release
monochange step tag-release --from HEAD --push=false
```

Verified behavior of each step:

- `monochange prepare` plans `1.0.1`, rewrites `packages/api/package.json`, renders
  `packages/api/CHANGELOG.md`, deletes the consumed changeset, and writes the durable record
  `.monochange/releases/<id>/release.json`. `.monochange/local/` artifacts are created too;
  monochange adds `.monochange/local/` to `.git/info/exclude` automatically, so `git status`
  stays clean after the release commit.
- `monochange step commit-release` creates the release commit
  `chore(release): prepare release`, whose body embeds the release record block. `tag-release`
  requires the resolved ref to be exactly this kind of commit.
- `monochange step tag-release --from HEAD --push=false` creates the tag `v1.0.1` on the
  release commit. `--push=false` is **required**: the default `--push=true` fails with exit 1
  when no remote is configured (`Please make sure you have the correct access rights`). The
  local tag is still created before the push fails, so rerunning with `--push=false` reports
  `already_up_to_date` and the sequence recovers without cleanup.

## 3. Second release: minor to v1.1.0 (proves the cycle repeats)

```sh
cat > .changeset/feat-duration-format.md <<'EOF'
---
"@acme/api": minor
---

Add `formatDuration` for human-readable retry timing in logs.
EOF

monochange prepare
monochange step commit-release
monochange step tag-release --from HEAD --push=false
```

## 4. Verification reads available to scenario checks

```sh
# Prints the release-record commit SHA for a tag.
monochange step release-record --sha --from v1.1.0

# Full record as JSON (status, packages, changelogs, provider).
monochange step release-record --from v1.1.0 --format json

# Registry readiness without publishing. Exits 0.
monochange step publish-readiness --from v1.0.1 --format json
```

- `monochange step release-record --from <tag>` resolves the record from the tag through the
  commit body; `--sha` prints only the commit SHA. `--from v1.0.1` and `--from v1.1.0` resolve
  to their own release commits.
- `monochange step publish-readiness --from <ref>` (equivalently
  `monochange publish readiness`) runs a dry-run and never publishes. For the fictional
  `@acme/api` it reports top-level `status: "blocked"` and per-package
  `status: "blocked"` with the message that the package has never been published to npm and
  suggests `monochange step placeholder-publish`. Scenario checks should expect `blocked`
  (or assert on `resolved_commit` / `record_commit` / `package_set_fingerprint`), not `ready`.
- `monochange step tag-release --from v1.0.0` deliberately fails: `no monochange release
  record found in first-parent ancestry from v1.0.0`, because the import tag is not a release
  commit. Only release commits created by `step commit-release` qualify.
- Moving tags later is `monochange step retarget-release --from <REF> --target <REF>`; it is
  not needed to build history.
