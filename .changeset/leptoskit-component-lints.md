---
monochange_app: none
---

# Enforce leptoskit component lint rules in CI

Website UI crates now run the official `leptoskit lint` rules (`component_attribute`, `markup_style` with `markup = "view"`) through `lint:leptoskit`, which is wired into `lint:all`, `lint:push`, and the CI lint job. Maintainers install `leptoskit_cli` via `cargo bin --install` (pinned in `[workspace.metadata.bin]`) and run `devenv shell lint:leptoskit` locally; each `leptoskit.toml` should deny those rules so `#[component]`, `view!` markup, and PascalCase component names stay required.
