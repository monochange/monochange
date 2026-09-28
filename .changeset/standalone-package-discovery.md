---
"monochange": patch
---

# Discover every standalone package that has no workspace root

Repositories that contain several independent packages without a workspace manifest between them only ever reported one package per ecosystem, because discovery gave each standalone manifest an id derived from its own directory:

```
before:  cargo:Cargo.toml, dart:pubspec.yaml, deno:deno.json, python:pyproject.toml
after:   cargo:crates/alpha/Cargo.toml, cargo:crates/beta/Cargo.toml, cargo:crates/gamma/Cargo.toml, ...
```

Every standalone manifest now produces an id relative to the discovery root, so `monochange step discover`, `monochange versions list`, release planning, change classification, and changeset-policy matching all see the full set. Given a repository with no root manifest:

```text
crates/alpha/Cargo.toml
crates/beta/Cargo.toml
crates/gamma/Cargo.toml
```

```bash
monochange step discover --format json
```

```json
{
	"packages": [
		{ "id": "cargo:crates/alpha/Cargo.toml", "name": "alpha" },
		{ "id": "cargo:crates/beta/Cargo.toml", "name": "beta" },
		{ "id": "cargo:crates/gamma/Cargo.toml", "name": "gamma" }
	]
}
```

Cargo, Dart, Deno, and Python discovery shared the defect and are all fixed. Go was unaffected because it already derived ids from the discovery root, and npm normalizes ids during discovery. Workspace members keep their existing ids, so a root `[workspace]` manifest with `members = ["crates/*"]` behaves exactly as before.
