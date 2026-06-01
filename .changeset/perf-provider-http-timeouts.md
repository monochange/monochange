---
monochange: patch
monochange_hosting: patch
monochange_python: patch
---

# Harden dark-area performance feedback

Provider API clients now set explicit connection and request timeouts so release and pull-request operations against GitLab, Gitea, Forgejo, and GitHub fail with context instead of appearing to hang indefinitely.

External command steps now emit heartbeat progress while a child process is still running but not producing stdout or stderr, giving users and agents feedback during slow lockfile, registry, and publish commands.

Discovery benchmarks now cover generated npm, Deno, Python, Go, and mixed-ecosystem repositories at 50, 100, and 500 packages.

Python discovery now skips `.egg-info` directories by suffix instead of treating `*.egg-info` as a literal directory name, avoiding unnecessary scans of package metadata in large Python repositories.
