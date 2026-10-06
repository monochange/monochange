<!-- {=projectCliAfterLongHelp} -->

### Quick Start

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
monochange discover --format json
```

4. Check what the next version will be before preparing a release. This reads pending changesets and writes nothing:

```bash
monochange next
monochange next --format json
```

5. Run repository-defined workflows as `monochange <command>` (or `monochange run <command>`, which also works when a built-in command shares the name), or call immutable built-in steps directly with `monochange step <name>`.

6. Preview before mutating files, publishing packages, creating tags, or opening release requests:

```bash
monochange run release --dry-run --diff
monochange preview --diff
```

Run `monochange help <command>` or `monochange help step <name>` for command-specific options.

<!-- {/projectCliAfterLongHelp} -->
