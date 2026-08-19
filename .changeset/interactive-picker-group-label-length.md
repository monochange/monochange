---
"monochange": patch
---

# Cap the group option label length in the interactive change picker

Groups can contain dozens of packages, and the picker previously inlined every package id into the option label. A label longer than the terminal width wraps onto multiple rows, which corrupts the picker's single-row-per-option layout: option scrolling and filter input echo stop working. Labels now inline the package ids only while the result stays short and fall back to `[group] <id> (<count> packages)` otherwise.
