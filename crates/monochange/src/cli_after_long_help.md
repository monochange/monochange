<!-- {=projectCliAfterLongHelp} -->

### Quick CLI workflow

1. Create or update `monochange.toml` for your workspace:

```bash
monochange init
```

2. Validate configuration and changeset targets before making release changes:

```bash
monochange step validate
```

3. Inspect detected package ids and groups when authoring changesets or workflow inputs:

```bash
monochange step discover --format json
```

4. Use repository-defined workflows through `monochange run <command>` when they exist in your config, or call immutable built-in steps directly with `monochange step <name>`.

5. Preview before mutating files, publishing packages, creating tags, or opening release requests:

```bash
monochange run release --dry-run --diff
monochange step prepare-release --dry-run --diff
```

Run `monochange help <command>` or `monochange help step <name>` for command-specific options.

<!-- {/projectCliAfterLongHelp} -->
