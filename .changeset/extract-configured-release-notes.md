---
monochange: minor
"@monochange/skill": minor
---

# extract one configured release-note output without preparing a release

The new read-only `monochange notes` command renders the stream and format selected by a named changelog output. It prints one artifact to stdout by default, accepts `--target` when an output covers multiple packages or groups, and writes only an explicitly requested `--file` path.

**Before:** automation had to parse the complete dry-run manifest or prepare configured changelog files to obtain one audience's notes.

```bash
monochange step prepare-release --dry-run --format json
```

**After:** select the configured artifact directly.

```bash
monochange notes --output user --target app
monochange notes --output user --target app --file artifacts/app-release-notes.md
```

Rendering does not update manifests, consume changesets, or write the configured changelog destination. The bundled agent skill documents how to choose stream-specific types, author separate developer and user changesets, validate them, and use extracted notes in reviews, CI, hosted releases, app-store releases, or patch delivery.
