---
monochange: fix
---

# Align text tables when names contain emoji or CJK characters

Text results measured column widths by counting characters, so a package or group id such as `🚀-launcher` or `docs-日本語` pushed every later column out of line. Widths are now measured in terminal columns with `unicode-width`, and previews of long lines are shortened by columns too, so wide names no longer split a character or overrun the limit.

`--progress-format ascii` (or `MONOCHANGE_PROGRESS_FORMAT=ascii`) now also keeps the result on stdout within ASCII. Headlines use the same symbols as the ASCII progress output (`+`, `!`, `x`, `-`), and the headline separator and truncated-list ellipsis become `-` and `...`:

```text
$ monochange next --progress-format ascii
+ Next versions - 3 packages

Packages
  core         0.1.1  patch
  docs-日本語  0.1.1  patch
  🚀-launcher  0.1.1  minor
```
