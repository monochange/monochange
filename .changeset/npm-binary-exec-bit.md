---
"@monochange/cli": fix
---

# Run the npm-installed binary when it lost its executable bit

Every `@monochange/cli` 0.20.0 platform package (`@monochange/cli-linux-x64-gnu`, `@monochange/cli-darwin-arm64`, and the rest) shipped `bin/monochange` with mode `0644`, so `npx @monochange/cli`, `npm install -g @monochange/cli`, and `pnpm exec monochange` failed on Linux and macOS with:

```text
Unable to find a compatible monochange binary in the installed npm packages.
@monochange/cli-linux-x64-gnu: spawnSync …/bin/monochange EACCES
```

The `monochange` launcher now checks the platform binary before running it. When the executable bit is missing, it restores it in place, or, when the install directory belongs to another user, runs a private executable copy from the temporary directory and removes it afterwards. Upgrade to this release; no configuration change is needed.
