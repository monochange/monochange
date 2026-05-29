---
monochange: patch
---

# Skip config loading for --version and --help flags

Previously, every CLI invocation loaded workspace configuration from disk
before parsing arguments. This meant `mc --version` and `mc --help` paid
the cost of reading and parsing monochange.toml even though they don't
need configuration.

The fix adds a fast path that parses arguments with the base command
(no config-loaded subcommands) first. If the result is --version or
root-level --help, it returns immediately without touching disk.

Benchmark results (release build, 50 runs each):

- --version: 8ms (was already fast in release, but avoids config I/O)
- --help: 8ms
- init --help: 9ms
- check --help: 9ms
- step:validate --help: 9ms

Also exports build_command_for_root for production use and adds
scripts/benchmark-commands.sh for PR regression detection.
