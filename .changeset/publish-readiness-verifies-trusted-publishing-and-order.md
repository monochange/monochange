---
monochange: minor
monochange_publish: minor
---

# publish readiness verifies trusted publishing and publish order

`monochange step publish-readiness` now verifies trusted publishing for every selected package before any registry state is mutated, and validates the planned publication order against the workspace dependency graph.

## Trusted publishing checks

For each package with `publish.trusted_publishing = true`, readiness now checks:

- the GitHub trust context (`publish.trusted_publishing.repository`, `workflow`, and the optional `environment`) resolves from configuration, source settings, or the CI environment
- the referenced workflow file exists under `.github/workflows/`
- the current environment can verify the CI/OIDC identity when a supported CI provider is detected; failures inside a supported provider block the package because the publish would fail outright
- the package exists on npm, crates.io, and pub.dev. These registries only accept trusted publishing for packages that already exist — a never-published package is blocked with guidance to bootstrap via `monochange step placeholder-publish`. Existing packages are reported as `manual_verification_required` because trusted publisher entries cannot be read back without registry credentials; the report includes the registry setup URL. Registry lookups are skipped (non-blocking) for registries without a public probe and when the network is unavailable.

Each package row in the readiness artifact gains a `trusted_publishing` finding with status `disabled`, `verified`, `manual_verification_required`, or `blocked`; blocked findings block the package and the overall report. Text and Markdown reports render the new column, and local (non-CI) runs degrade identity checks to `manual_verification_required` so the report stays useful during development.

The artifact schema advances from version 2 to 3; regenerate readiness artifacts after upgrading.

## Publication order checks

The artifact now records the dependency-corrected `publish_order` and `order_findings`. The readiness step independently validates the planned order against the workspace dependency graph (including dev-dependencies): a package scheduled before one of its workspace dependencies is a blocking order finding that marks the package as blocked. A release record whose recorded publication order differs from the corrected plan is a non-blocking note, because `publish-packages` follows the dependency-corrected order.

## Publish-run preflight

Real `monochange step publish-packages` runs now run every readiness checker for all packages before the first publish command executes. A package that cannot publish aborts the run before any registry mutation, instead of failing midway after other packages have already been published. Trusted-publishing project-side checks are registered for every built-in registry, so a package whose trust configuration cannot support the publish is caught in preflight too.

`monochange_publish::registry_client` now bounds every registry request with a 30-second connect timeout and a 60-second total timeout, so an unresponsive registry surfaces as a handled error instead of stalling a publish run or readiness check indefinitely.

New `monochange_publish` APIs: `registry_package_exists_with_transport` probes package existence on npm, crates.io, and pub.dev; `publish_order_dependency_edges` and `publish_order_dependency_fields` are now public for order validation.
