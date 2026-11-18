---
monochange_app: fix
---

# Reduce compressed browser module size

Release web builds now remove compiler metadata and compare additional Binaryen optimizations before assembling and precompressing both browser entries. Each candidate must preserve the module's imports and exports, reduce the size gate's gzip transfer, and avoid increasing deployed gzip or brotli bytes. Invalid output and compiler failures stop the build. Rebuild both entries through the existing leptoskit web-assets plugin; the already-required `wasm-opt` supplies the optimization passes.
