---
monochange: feat
"@monochange/skill": feat
---

# Add `monochange completions` and `monochange doctor`

Two new commands close the two most common first-hour gaps:

- `monochange completions <bash|zsh|fish|elvish|powershell>` prints a shell completion script. The script is generated from the commands the current repository's `monochange.toml` defines, so config-defined workflows complete alongside the built-ins. Install it with your shell's completion mechanism, for example `monochange completions zsh > "${fpath[1]}/_monochange"`.
- `monochange doctor` checks the local environment in one place: git availability, `monochange.toml` parsing, and the toolchain behind every ecosystem enabled in the workspace (`cargo`, `node`, `deno`, `dart`, `python3`, `go`, `swift`, `pod`, `gradle`). Each check reports the tool's version or how to install it, with `--format json` for tooling. Checks are local only — nothing contacts the network.
