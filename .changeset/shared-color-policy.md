---
"monochange": feat
---

# Add a global `--color` flag and one color policy for all output

Every command now accepts `--color <auto|always|never>`, and results on stdout, progress, diagnostics, and warnings on stderr, release-file diffs, and `--help` pages all follow the same rule. Before, each output decided on its own: only diffs honored `CLICOLOR_FORCE`, result text stayed plain whenever `TERM` was unset, and `--help` was never colored.

`always` and `never` override the environment. Under `auto` (the default):

- a non-empty `NO_COLOR` turns color off;
- otherwise `CLICOLOR_FORCE` set to anything but `0` turns it on, even when output is piped;
- otherwise `CLICOLOR=0` or `TERM=dumb` turn it off;
- otherwise only a terminal gets color.

```bash
# keep colors when piping into a pager
monochange --color always step validate | less -R

# plain output in a terminal
monochange --color never next
```

Progress step labels are now bold in the terminal's default color instead of bold white, so they stay readable on light themes, and erasing the spinner line no longer writes a color reset when color is off.
