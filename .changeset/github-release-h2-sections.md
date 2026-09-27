---
"monochange_hosting": patch
"monochange_github": patch
---

# Use h2 sections without version titles in provider release bodies

Provider release bodies duplicated the release title (`## <version>`) that already ships as the release `name`, and rendered sections as `###` with expanded entries as `####`.

The body now drops the version title header and promotes one level to match knope-style releases where the body starts at `## <section>`:

```markdown
Before (body duplicated the release name):

## sdk 1.2.0 (2026-04-06)

Group summary

### Features

- group feature

After (title lives in the release name, body starts at h2):

Group summary

## Features

- group feature
```

The release `name` is unchanged and still includes the date by default. Grouped fallbacks also drop the title and the member wrapper, listing each member package as an h2 section with its own h3 subsections. Changelog files keep the version title and h3 sections.
