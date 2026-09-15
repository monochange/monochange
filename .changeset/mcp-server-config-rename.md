---
monochange: patch
---

# Follow the rmcp 3.4 rename from ServerInfo to ServerConfig

The MCP server implements `ServerHandler::get_info` with the type alias `rmcp::model::ServerInfo`, which rmcp 3.4 deprecates in favor of `ServerConfig`. Because `monochange run release --commit` re-locks dependencies before the pre-merge lint, any prepared release now compiles against rmcp 3.4 and the deprecated alias failed `lint:clippy` under `-D warnings`.

The implementation and its test now name `ServerConfig` directly, which is the same type under the non-deprecated name. No MCP tool, response shape, or serialized field changed.
