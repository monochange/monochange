---
"monochange_core": fix
---

# Emphasize only the lead sentence of multi-sentence release-note summaries

Grouped release notes used to render a changeset's whole first line in bold, so summaries that describe an entire change in one sentence-packed line became dense bold blocks in changelogs and release PR bodies. Readers lost the visual anchor that the bold lead was supposed to provide.

Compact entries now emphasize only the first sentence; the remaining sentences keep the plain formatting they had in the changeset file. Expanded entries move those sentences out of the `####` heading into the first body paragraph. Sentence boundaries are detected conservatively: periods inside inline code spans, in abbreviations such as `e.g.`, in version numbers like `3.13.0`, and after single-letter initials never end a sentence, and punctuation followed directly by more text such as `Node.js` never splits.

Multi-package compact entries also regain the missing space between the package list and the summary:

**Before:**

```text
- _Packages:_ _core_, _app_**Add shared release note.**
```

**After:**

```text
- _Packages:_ _core_, _app_ **Add shared release note.**
```
