---
monochange: patch
---

# Add JSON schema annotation to init template

The generated `monochange.toml` now includes a `#:schema` directive at the top, enabling automatic validation in editors that support JSON Schema annotations (e.g., VS Code with Even Better TOML).
