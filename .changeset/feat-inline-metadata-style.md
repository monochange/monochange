---
monochange_core: minor
monochange_changelog: minor
monochange: patch
---

#### add `Inline` metadata style and make it the default

Context blocks in changelog entries now render as a single inline paragraph joined with `·` instead of separate lines.

When a review request (PR/MR) link is available, commit links are omitted since the PR already identifies the change. When no review request link exists, commit links are included as before.

The existing `Plain` and `Blockquote` styles continue to render commit links unconditionally. The `Omit` style hides all metadata as before.

#### Before (default: `plain`)

```markdown
#### Add release summary panel

_Owner:_ @user _Review:_ [PR #123](https://...) _Introduced in:_ [`abc1234`](https://...) _Related issues: #456
```

#### After (default: `inline`)

```markdown
#### Add release summary panel

_Owner:_ @user · _Review:_ [PR #123](https://...) · _Related issues: #456
```

Set `metadata_style = "inline"` (now the default), `"plain"`, `"blockquote"`, or `"omit"` under `[changelog.style]` in `monochange.toml`.
