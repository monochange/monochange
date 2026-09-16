---
"monochange": patch
---

# Generate the schema version list on the docs reference page

`docs/src/reference/schemas.md` listed the current and versioned schema URLs by hand, so every new contract version needed a manual edit and the page could drift from what is actually hosted.

The version list is now generated from the committed versioned schema assets. `scripts/schema-versions.ts` reads `docs/src/schemas/`, groups the assets by family, and keeps only versions on the breaking axis: while a family's major version is `0` every minor bump may break consumers, so each `0.N` is published, and from `1.0` onward only `N.0` is. Git tags are deliberately not the source: not every schema family is tagged, and CI checks out shallowly where tags are absent entirely.

The script is wired into the docs through an `[data]` entry in `mdt.toml`, so `mdt update` and `docs:update` regenerate the page and `mdt check` fails if it drifts.

Two supporting changes were needed to keep `mdt check` green after `dprint fmt`:

- `.templates/` is excluded from dprint. dprint's markdown formatter treats jinja tags as ordinary prose, reflowing them onto adjacent lines and inserting blank lines after them, which mdt then renders into consumer blocks.
- Provider blocks that document literal `{{ ... }}` or GitHub Actions `${{ ... }}` syntax are wrapped in `{% raw %}...{% endraw %}`. Adding any `[data]` entry turns on template rendering for every provider block, so without the wrapper minijinja renders those documented examples down to empty strings, and `mdt check` still passes on the emptied result. The wrappers are byte-neutral, so no rendered page content changed.
