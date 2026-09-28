# Empty workspace

No package manifests, and that is the point: this fixture pins what
`monochange init` generates on a bare repository. Fixtures are committed
without `.git`, so scenarios that need a real repository declare their own
`setupCommands` (for example `git init` plus an initial commit).
