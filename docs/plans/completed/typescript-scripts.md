# TypeScript scripts

## Status

Completed and archived after `refactor: migrate scripts to TypeScript (#529)` landed.

## Goal

Convert the repository JavaScript tooling and npm CLI launcher source to TypeScript in an isolated worktree, using native Node TypeScript execution on Node 24 instead of adding `tsx`.

## Decisions

- Use `.ts` for executable repository scripts and script tests, with root `type: module` so Node treats them as ESM.
- Keep the published npm CLI bin as `packages/monochange__cli/bin/monochange.js` by building it from `packages/monochange__cli/src/monochange.ts` with `tsdown` alone, using ESM output and `fixedExtension: false`.
- Run `pnpm build` before node script tests and before the npm package population step in publishing.
- Add `prepack` and `prepublishOnly` for `@monochange/cli` so packaging and publishing build the CLI launcher first.
- Do not use `// @ts-nocheck`; keep migrated scripts type-checkable under the repository `tsconfig.json`.
- Run TypeScript type-checking from both `lint:all` and the CI `lint` job so script type regressions are normal lint failures.

## Progress

- [x] Created branch/worktree `refactor/typescript-scripts`.
- [x] Verified Node 24 runs `.ts` scripts natively in an ESM package, including shebang scripts.
- [x] Renamed repository `.mjs` scripts and Vitest files to `.ts`.
- [x] Moved the npm CLI launcher source from `bin/monochange.js` to `src/monochange.ts`.
- [x] Configured `tsdown` to build the generated ESM CLI launcher directly into `packages/monochange__cli/bin/monochange.js`.
- [x] Updated workflows, docs, tests, and devenv scripts to reference `.ts` files.
- [x] Converted example and fixture `.js` source files to `.ts`.
- [x] Added `@types/node` and updated the lockfile.
- [x] Added a changeset for `@monochange/cli`.
- [x] Validated formatting, type-checking, linting, build, and node tests locally.
- [x] Removed all `// @ts-nocheck` comments and fixed the resulting TypeScript diagnostics.
- [x] Addressed PR review by raising the `tsdown` target to `node22`.
- [x] Added an explicit CI lint step for `lint:js:types`; `lint:all` runs the same type-check script before workflow scanning.
- [x] Made local `lint:workflows` include Cargo's install bin directory so `lint:all` can find a cargo-binstalled `zizmor`.
- [x] Addressed PR review by switching repository scripts/tests from `.mts` to `.ts`, adding `type: module`, keeping `tsdown` on ESM output, and simplifying `pnpm build` to plain `tsdown`.

## Validation

- `devenv shell pnpm fmt:check`
- `devenv shell pnpm check`
- `devenv shell pnpm lint:syntax`
- `devenv shell pnpm lint`
- `devenv shell pnpm build`
- `devenv shell test:node`

Devenv emitted FlakeHub cache 401 warnings during these commands; they did not affect command results.

Additional review-follow-up validation:

- `pnpm fmt:check`
- `pnpm check`
- `pnpm lint`
- `devenv shell lint:js:types`
- `pnpm build`
- `devenv shell test:node`
- `devenv shell mc step:validate`
- `git diff --check`

`devenv shell lint:all` now runs `lint:js:types` before workflow scanning; it reaches and passes TypeScript checking, then fails later on pre-existing `zizmor` workflow findings unrelated to the TypeScript migration.

Second PR-review follow-up validation after switching scripts/tests to `.ts` and simplifying `pnpm build` to `tsdown`:

- `pnpm fmt`
- `pnpm fmt:check`
- `pnpm check`
- `pnpm lint:syntax`
- `pnpm lint`
- `pnpm build`
- `devenv shell test:node`
- `devenv shell mc step:validate`
- `devenv shell lint:format`
- `git diff --check`
